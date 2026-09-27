//! Ordinary std::net TCP functions over loopback peers.
//!
//! The same linked implementation may use the native engine or file adapter.
//! These private choices preserve bytes and outcomes. C2 removed PAR-3, so
//! the previous reverse-peer scheduling assertion and staged-lane shape
//! assertion are retired; the source fanout loop now serves peers in order.
//! Windows selects the existing hosted echo/refusal scope. The remaining
//! POSIX scenarios keep their existing collection; porting a harness does not
//! silently add another host matrix for every historical case.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

#[cfg(unix)]
use whitefoot::{CompilerLimits, OverlapLowering, SourceInput};

use super::support::{
    CompiledProgram, ProgramChild, build_program, compile_program, compile_program_with_overlap,
};
#[cfg(unix)]
use super::support::{
    compile_and_run, compile_program_without_overlap, emitted_function, program_permission_ledger,
};

/// One port the host is not using, released before the program binds it.
///
/// A listening socket that never accepted leaves no connection in `TIME_WAIT`,
/// so the port is free the moment this drops and the program's own `bind`
/// answers without `SO_REUSEADDR` — which the runtime deliberately does not
/// set, because it would change what a second bind of one port means
///.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve a loopback port");
    listener
        .local_addr()
        .expect("the reserved port's address")
        .port()
}

/// Connects to a program that is still starting.
///
/// The program binds its listener some time after the harness spawned it, so
/// the first attempts are refused. This retries for a bounded wall-clock span
/// and fails the case if the program never listened; nothing about the
/// program's own acceptance depends on it.
fn connect_when_ready(port: u16) -> TcpStream {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            Ok(stream) => return bounded_stream(stream),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("the program never listened on {port}: {error}"),
        }
    }
}

fn bounded_stream(stream: TcpStream) -> TcpStream {
    stream.set_nonblocking(false).expect("blocking peer socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("bound peer reads");
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("bound peer writes");
    stream
}

#[cfg(unix)]
fn accept_when_ready(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("bound listener acceptance");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return bounded_stream(stream),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("client did not connect within the test deadline: {error}"),
        }
    }
}

/// The exit code one finished child reported, with its diagnostics on failure.
fn finished(child: ProgramChild) -> (i32, Vec<u8>) {
    let output = child.wait_with_output().expect("wait for compiled program");
    assert!(
        output.stderr.is_empty(),
        "native program diagnostics: {output:?}"
    );
    (output.status.code().unwrap_or(-1), output.stdout)
}

fn payload() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(10_000);
    for index in 0..10_000_u32 {
        bytes.push(u8::try_from(index % 251).expect("a byte"));
    }
    bytes
}

/// Runs one echo exchange against `tcp_echo.wf` and returns what came back
/// beside the program's own status.
fn echo_exchange(program: &CompiledProgram, native_ring: bool, bytes: &[u8]) -> (Vec<u8>, i32) {
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
    let mut stream = connect_when_ready(port);
    stream.write_all(bytes).expect("send the payload");
    // The connection's receiving direction ends where this peer stops sending,
    // and the server's `receive_next` answers `ReadEnd` for exactly that
    //.
    stream.shutdown(Shutdown::Write).expect("stop sending");
    let mut returned = Vec::new();
    stream
        .read_to_end(&mut returned)
        .expect("read the echoed bytes");
    drop(stream);
    let (status, _) = finished(child);
    (returned, status)
}

#[cfg(unix)]
#[test]
fn ipv4_checksum_uses_one_slice_consumer_for_static_and_runtime_storage() {
    let llvm = compile_program("ipv4_checksum.wf");
    let checksum = emitted_function(&llvm, "ipv4_checksum");
    let main = emitted_function(&llvm, "main");
    // The discharged slice reads emit no bounds branch; the loop invariants
    // establish the address domains before the element addresses form.
    assert!(checksum.contains("getelementptr inbounds i8"));
    assert!(!checksum.contains("call void @free"));
    assert_eq!(main.matches("call i16 @wf_ipv4_checksum").count(), 2);
    // B7c4b-1: the runtime copy of the header is a run taken from one bump
    // extent reserved in this activation's frame, so the program reaches the
    // host allocator on no path at all and every validation-failure return
    // leaves the extent with the frame. The free this assertion used to count
    // was the heap buffer's, and there is no heap buffer any more.
    assert!(!main.contains("call void @free"));
    assert!(!llvm.contains("call ptr @malloc"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn tcp_calls_use_ordinary_linked_declarations() {
    let llvm = compile_program("tcp_echo.wf");
    for name in [
        "tcp_listen",
        "tcp_accept",
        "receive_next",
        "send_once",
        "close_listener",
        "close_receive",
        "close_send",
    ] {
        assert!(
            llvm.contains(&format!("call void @wf_std.net.{name}(")),
            "missing ordinary call {name}"
        );
        assert!(
            llvm.contains(&format!("declare void @wf_std.net.{name}(")),
            "missing ordinary declaration {name}"
        );
    }
    assert!(!llvm.contains("@wf__completion_"));
}

#[test]
fn a_loopback_echo_preserves_all_bytes_and_half_close_on_both_routes() {
    let bytes = payload();
    for parallel in [false, true] {
        let llvm = if parallel {
            compile_program_with_overlap("tcp_echo.wf")
        } else {
            compile_program("tcp_echo.wf")
        };
        let program = build_program(&llvm);
        for native_ring in [true, false] {
            for bytes in [bytes.as_slice(), &[]] {
                let (returned, status) = echo_exchange(&program, native_ring, bytes);
                assert_eq!(
                    status, 0,
                    "parallel: {parallel}, native ring: {native_ring}"
                );
                assert_eq!(
                    returned, bytes,
                    "parallel: {parallel}, native ring: {native_ring}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_peer_that_resets_reaches_the_program_as_its_own_outcome_on_both_routes() {
    let llvm = compile_program("tcp_echo.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
        let mut stream = connect_when_ready(port);
        // Small enough that this peer's own send completes without waiting
        // for anything, and never read back: the program echoes it into this
        // socket's receive queue, and a host closing a connection whose
        // receive queue still holds data sends a reset rather than a graceful
        // end. That is the reset the program then observes. The peek is what
        // makes the queue hold data at the close: a close that raced ahead of
        // the echo would send a graceful end instead, which the program then
        // reads as the direction's end before any reset reaches it, and a
        // receive the submitting thread answers at once made that race real
        // on the macOS runner.
        let bytes = vec![7_u8; 64 * 1024];
        stream.write_all(&bytes).expect("send the payload");
        stream
            .peek(&mut [0_u8; 1])
            .expect("the first echoed byte arrives before the peer closes");
        drop(stream);
        let (status, _) = finished(child);
        // `tcp_echo.wf` reports 20 plus the portable class for a refused
        // receive and 30 plus it for a refused send; class 2 is
        // `ConnectionReset` and class 4 is `BrokenPipe`. Which of the
        // three the program observes is the host's own timing and every one of
        // them is the peer's reset reaching source as an ordinary outcome.
        assert!(
            matches!(status, 22 | 32 | 34),
            "a reset must reach source as ConnectionReset or BrokenPipe, got {status} \
             (native ring: {native_ring})"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_whitefoot_client_sends_and_receives_on_both_routes() {
    let llvm = compile_program("tcp_client.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listen for the client");
        let port = listener.local_addr().expect("the listening address").port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
        let mut stream = accept_when_ready(&listener);
        let mut sent = [0_u8; 8];
        stream
            .read_exact(&mut sent)
            .expect("read what the client sent");
        assert_eq!(&sent, b"ABCDEFGH", "native ring: {native_ring}");
        stream.write_all(b"abcdefgh").expect("answer the client");
        // The client reads to the end of its receiving direction, which this
        // peer decides.
        stream.shutdown(Shutdown::Write).expect("stop sending");
        let (status, published) = finished(child);
        assert_eq!(status, 0, "native ring: {native_ring}");
        assert_eq!(published, b"abcdefgh", "native ring: {native_ring}");
    }
}

#[cfg(unix)]
#[test]
fn four_connections_reach_one_listener_on_both_routes() {
    let llvm = compile_program("tcp_fanout.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
        for peer in 0..4_u8 {
            let mut stream = connect_when_ready(port);
            let sent = [peer, peer + 1, peer + 2];
            stream.write_all(&sent).expect("send this peer's bytes");
            let mut returned = Vec::new();
            stream
                .read_to_end(&mut returned)
                .expect("read this peer's answer");
            assert_eq!(returned, sent, "peer {peer} (native ring: {native_ring})");
        }
        let (status, _) = finished(child);
        assert_eq!(status, 0, "native ring: {native_ring}");
    }
}

#[cfg(unix)]
#[test]
fn the_fanout_loop_has_only_ordinary_counted_permission() {
    // PAR-2 checks the explicit ordinary close/serve statements under its
    // normal conditions. Deleted PAR-3 supplies no second judgment.
    //
    // The serve loop's `close_listener` expression statement is judged by its
    // call's row, exactly as a let-bound call is, so the loop is no longer
    // refused for that spelling. It is refused for what it does: the first
    // condition it fails is the reported one, the `outcome` it carries between
    // iterations, and `serve_one` also writes the shared listener and factory.
    let ledger = program_permission_ledger("tcp_fanout.wf");
    assert!(
        ledger.iter().any(|line| line.starts_with("PAR loop")
            && line.contains("denied")
            && line.contains("condition 1:")
            && line.contains("set outcome = reported;")),
        "{ledger:?}"
    );
    assert!(
        !ledger
            .iter()
            .any(|line| line.contains("the body contains an expression statement")),
        "{ledger:?}"
    );
    assert!(
        !ledger
            .iter()
            .any(|line| line.starts_with("PAR stage") || line.starts_with("PAR place")),
        "{ledger:?}"
    );
}

#[test]
fn two_refused_connects_leave_the_factory_usable_on_both_routes() {
    let llvm = compile_program("tcp_refused.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        // A port this process reserved and released: nothing is listening on
        // it, so the host answers the connect with its own refusal.
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
        let (status, _) = finished(child);
        // Both attempts must report ConnectionRefused; failed construction
        // leaves the same factory available for the second ordinary call.
        let reservation = TcpListener::bind(("127.0.0.1", port))
            .expect("refusal fixture's released port was claimed by another process");
        assert_eq!(
            status, 0,
            "native ring: {native_ring}; this is a released-port fixture, not a credit-count assertion"
        );
        drop(reservation);
    }
}

/// Four accepted connections must
/// still be served correctly under --par on native and helper routes, with
/// peers speaking in acceptance order. The earlier reverse-order test was
/// specifically a managed-stack concurrency requirement; this does not claim
/// the same head-of-line-blocking behavior or throughput.
#[cfg(unix)]
#[test]
fn four_peers_are_served_in_order_under_par_on_both_routes() {
    let llvm = compile_program_with_overlap("tcp_fanout.wf");
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route_with_workers(native_ring, Some("3"), &[text.as_bytes()]);
        let mut streams = (0..4_u8)
            .map(|_| connect_when_ready(port))
            .collect::<Vec<_>>();
        for peer in 0..4_u8 {
            let stream = &mut streams[usize::from(peer)];
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .expect("bound the wait for this peer's answer");
            let sent = [peer, peer + 1, peer + 2];
            stream.write_all(&sent).expect("send this peer's bytes");
            let mut returned = Vec::new();
            stream.read_to_end(&mut returned).unwrap_or_else(|error| {
                panic!(
                    "peer {peer} was not answered in acceptance order \
                     (native ring: {native_ring}): {error}"
                )
            });
            assert_eq!(returned, sent, "peer {peer} (native ring: {native_ring})");
        }
        drop(streams);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "native ring: {native_ring}");
    }
}

/// Ordinary PAR-2 body-shape rules leave this fanout loop sequential; no
/// suspension classification decides which calls may be handed out.
#[cfg(unix)]
#[test]
fn the_fanout_loop_keeps_denied_calls_on_the_current_stack() {
    let overlapped = compile_program_with_overlap("tcp_fanout.wf");
    let main = emitted_function(&overlapped, "main");
    assert!(main.contains("@wf_serve_one("));
    assert!(!main.contains("@wf__par_publish("));
    assert!(!main.contains("par.staged."));

    let sequential = compile_program_without_overlap("tcp_fanout.wf");
    for entry in [
        "@wf__par_acquire_lane",
        "@wf__par_publish",
        "@wf__par_join",
        "@wf__par_release",
    ] {
        assert!(
            !sequential.contains(entry),
            "the --no-overlap module must name no lane entry, found {entry}"
        );
    }
}

// Reconstruct two ordinary structs from unrelated halves, then close each
// in a different order. The surviving cross must still exchange its bytes.
//
// Ported to v0.60: the unique-reference marker `&uniq` went with [OWN-2], the
// `region` blocks with [OWN-3] and [FORM-8], and `fixed_vector`, `slice_of`
// and `mut_slice_of` with [OP-1]'s retired rows. The one-byte scratch is now
// an inline `Slots<u8, 1>` reached through a range reference [TYPE-9, REF-4],
// a reference parameter is handed on as itself, and each effect occurrence is
// its own `writes` entry [EFF-1]. The exchange, the four close orders and
// every status code the test reads are unchanged.
#[cfg(unix)]
const CROSSED_CONNECTIONS: &str = r#"fn cross(first: std::net::TcpConnection, second: std::net::TcpConnection) -> (a: std::net::TcpConnection, b: std::net::TcpConnection) pure {
  let std::net::TcpConnection(receive: first_receive, send: first_send) = move first;
  let std::net::TcpConnection(receive: second_receive, send: second_send) = move second;
  let a = std::net::TcpConnection(receive: move first_receive, send: move second_send);
  let b = std::net::TcpConnection(receive: move second_receive, send: move first_send);
  return move a, move b;
}

fn close_pair(factory: &std::io::HandleFactory, connection: std::net::TcpConnection, receive_first: Bool) -> result: u8 writes(factory) {
  let std::net::TcpConnection(receive: receive, send: send) = move connection;
  let failed = 0_u8;
  if receive_first {
    match std::net::close_receive(factory: factory, receive: move receive) {
      Ok(value: done) => {
      }
      Err(error: problem) => {
        set failed = 1_u8;
      }
    }
    match std::net::close_send(factory: factory, send: move send) {
      Ok(value: done) => {
      }
      Err(error: problem) => {
        set failed = 2_u8;
      }
    }
  } else {
    match std::net::close_send(factory: factory, send: move send) {
      Ok(value: done) => {
      }
      Err(error: problem) => {
        set failed = 3_u8;
      }
    }
    match std::net::close_receive(factory: factory, receive: move receive) {
      Ok(value: done) => {
      }
      Err(error: problem) => {
        set failed = 4_u8;
      }
    }
  }
  return failed;
}

fn remaining(connection: &std::net::TcpConnection) -> result: u8 writes(connection.receive), writes(connection.send) {
  let bytes = slots_new::<u8, 1>();
  place_back(window: &bytes, value: 0_u8);
  let destination = &bytes[0_u64..1_u64];
  match std::net::receive_next(receive: &deref(connection).receive, destination: destination, start: 0_u64, end: 1_u64) {
    Ok(value: received) => {
      if received != 1_u64 {
        return 11_u8;
      }
    }
    Err(error: problem) => {
      return 12_u8;
    }
  }
  if bytes[0_u64] != 66_u8 {
    return 13_u8;
  }
  set bytes[0_u64] = 65_u8;
  let source = &bytes[0_u64..1_u64];
  match std::net::send_once(send: &deref(connection).send, source: source, start: 0_u64, end: 1_u64) {
    Ok(value: sent) => {
      if sent != 1_u64 {
        return 14_u8;
      }
    }
    Err(error: problem) => {
      return 15_u8;
    }
  }
  return 0_u8;
}

fn exercise(factory: &std::io::HandleFactory, address: &std::net::SocketAddress) -> result: u8 reads(address), writes(factory) {
  let receive_first = True();
  let send_first = False();
  match std::net::tcp_connect(factory: factory, address: address) {
    Ok(value: first) => {
      match std::net::tcp_connect(factory: factory, address: address) {
        Ok(value: second) => {
          let (a, b) = cross(first: move first, second: move second);
          let first_status = close_pair(factory: factory, connection: move a, receive_first: receive_first);
          let exchange_status = remaining(connection: &b);
          let second_status = close_pair(factory: factory, connection: move b, receive_first: send_first);
          if first_status != 0_u8 {
            return 21_u8;
          }
          if second_status != 0_u8 {
            return 22_u8;
          }
          if exchange_status != 0_u8 {
            return exchange_status;
          }
          match std::net::tcp_connect(factory: factory, address: address) {
            Ok(value: checkpoint) => {
              let checkpoint_status = remaining(connection: &checkpoint);
              let closed = close_pair(factory: factory, connection: move checkpoint, receive_first: receive_first);
              if closed != 0_u8 {
                return 25_u8;
              }
              return checkpoint_status;
            }
            Err(error: problem) => {
              return 26_u8;
            }
          }
        }
        Err(error: problem) => {
          close_pair(factory: factory, connection: move first, receive_first: receive_first);
          return 23_u8;
        }
      }
    }
    Err(error: problem) => {
      return 24_u8;
    }
  }
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: handles, stdin: input) = move inputs;
  let address = std::net::socket_address_v4(a: 127_u8, b: 0_u8, c: 0_u8, d: 1_u8, port: 49151_u16);
  std::fs::close_directory(factory: &handles, directory: move cwd);
  let outcome = exercise(factory: &handles, address: &address);
  return std::process::exit_status(code: outcome);
}
"#;

#[cfg(unix)]
#[test]
fn crossed_ordinary_tcp_halves_keep_the_other_directions_live() {
    for overlap in [None, Some(OverlapLowering::Off), Some(OverlapLowering::On)] {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("listen for both ordinary connections");
        listener
            .set_nonblocking(true)
            .expect("bound the wait for both connections");
        let port = listener.local_addr().expect("the listener address").port();
        let source = CROSSED_CONNECTIONS.replace("49151_u16", &format!("{port}_u16"));
        let inputs = [SourceInput::new("crossed.wf", source.as_bytes())];
        let llvm = match overlap {
            None => whitefoot::compile(&inputs, CompilerLimits::default()),
            Some(overlap) => {
                whitefoot::compile_with_overlap(&inputs, CompilerLimits::default(), overlap)
            }
        }
        .expect("ordinary construction and cleanup of crossed halves must compile");
        let program = build_program(&llvm);
        for native_ring in [true, false] {
            let mut child = program.spawn_on_route(native_ring, &[]);
            let mut accept = || {
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream
                                .set_nonblocking(false)
                                .expect("read accepted sockets in blocking mode");
                            stream
                                .set_read_timeout(Some(Duration::from_secs(20)))
                                .expect("bound peer reads");
                            stream
                                .set_write_timeout(Some(Duration::from_secs(20)))
                                .expect("bound peer writes");
                            break stream;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "connection did not arrive");
                            assert!(
                                child.try_wait().expect("check the child").is_none(),
                                "the program exited before connecting"
                            );
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("accept an ordinary connection: {error}"),
                    }
                }
            };
            let mut first = accept();
            let mut second = accept();
            // The first crossed struct is gone before WF reads this byte from
            // the second connection and sends 'A' on the first. Closing a
            // crossed struct as one original socket would break this exchange.
            second
                .write_all(b"B")
                .expect("send to the surviving receive half");
            // WF opens this checkpoint only after both crossed pairs close.
            // It must wait here for another B before it can exit, so teardown
            // cannot stand in for the two EOF observations below.
            let mut checkpoint = accept();
            let mut first_bytes = Vec::new();
            first
                .read_to_end(&mut first_bytes)
                .expect("read the surviving send half through its close");
            let mut second_bytes = Vec::new();
            second
                .read_to_end(&mut second_bytes)
                .expect("observe the other send half's close");
            assert_eq!(first_bytes, b"A", "{overlap:?}, native ring {native_ring}");
            assert!(second_bytes.is_empty());
            checkpoint
                .write_all(b"B")
                .expect("release the post-close checkpoint");
            let mut checkpoint_bytes = Vec::new();
            checkpoint
                .read_to_end(&mut checkpoint_bytes)
                .expect("read the checkpoint exchange");
            assert_eq!(checkpoint_bytes, b"A");
            let output = child
                .wait_with_output()
                .expect("wait for the crossed-half program");
            assert!(
                output.status.success(),
                "{overlap:?}, {native_ring}: {output:?}"
            );
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}
