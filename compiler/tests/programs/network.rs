//! Ordinary std::net TCP functions over loopback peers.
//!
//! The same linked implementation may use the native engine or file adapter.
//! These private choices preserve bytes and outcomes. C2 removed PAR-3, so
//! the previous reverse-peer scheduling assertion and staged-lane shape
//! assertion are retired; the source fanout loop now serves peers in order.
//! Windows selects the existing hosted echo/refusal scope and the context
//! cases. The remaining POSIX scenarios keep their existing collection;
//! porting a harness does not silently add another host matrix for every
//! historical case.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

#[cfg(unix)]
use whitefoot::{CompilerLimits, OverlapLowering, SourceInput};

#[cfg(target_os = "linux")]
use super::support::compile_app;
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
        // A waiting host function is linked as its start and its finish, the
        // two halves a waiting frame suspends between
        // (design/compiler/waiting-contexts.md); nothing calls a
        // blocking whole.
        assert!(
            llvm.contains(&format!("call i32 @wf_std.net.{name}.start(")),
            "missing ordinary start {name}"
        );
        assert!(
            llvm.contains(&format!("call void @wf_std.net.{name}.finish(")),
            "missing ordinary finish {name}"
        );
        assert!(
            llvm.contains(&format!("declare i32 @wf_std.net.{name}.start(")),
            "missing ordinary declaration {name}"
        );
        assert!(
            !llvm.contains(&format!("@wf_std.net.{name}(")),
            "a blocking call of {name}"
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
    // refused for that spelling. It is refused for what it does. Before
    // v0.77 the reported condition was the `outcome` it carries between
    // iterations; `serve_one` now waits [WAIT-1], and a body holding a waiting
    // call is refused by that condition first [PAR-2].
    let ledger = program_permission_ledger("tcp_fanout.wf");
    assert!(
        ledger.iter().any(|line| line.starts_with("PAR loop")
            && line.contains("denied")
            && line.contains("condition 5:")
            && line.contains("the waiting call serve_one(")),
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
        let child =
            program.spawn_on_route_with(native_ring, &[("WF_WORKERS", "3")], &[text.as_bytes()]);
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

/// Each accepted connection is served by a context of its own [WAIT-3], so a
/// peer is answered while every peer accepted before it is still silent.
/// The peers speak in the reverse of their acceptance order: a server that
/// served one connection at a time would wait on the first, silent peer and
/// never answer the last, and the read timeout would fail this case. Both
/// routes are required: with no ring, a context's socket wait is a readiness
/// wait rather than a blocking call on the one thread every context shares.
/// There are more peers than the helper pool has threads, so a runtime that
/// sent those waits to blocking helpers would hold only as many silent peers
/// as it has helpers and fail here too (`WAITS.md`, the readiness route).
/// Windows has no readiness wait, so there a context's socket wait without
/// the completion port is exactly such a helper wait, and only the port's
/// route runs (`docs/todo.md`, "Only Linux with a ring runs several
/// drivers").
/// [PRE-2] a deadline ends an accept no client answers and a receive the peer
/// never feeds, each only once the clock has reached it and with nothing
/// transferred: a byte sent afterwards arrives whole, and a sleeping context
/// and a bounded receive of the root both end. The program reports the first
/// check that failed as its status.
#[test]
fn a_passed_deadline_ends_a_wait_and_loses_nothing_on_both_routes() {
    let llvm = compile_program("deadlines.wf");
    let program = build_program(&llvm);
    let routes: &[bool] = if cfg!(windows) {
        &[true]
    } else {
        &[true, false]
    };
    for &native_ring in routes {
        let port = free_port();
        let text = port.to_string();
        let started = Instant::now();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes()]);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "native ring: {native_ring}");
        // Three deadlines of 50, 50 and 60 milliseconds passed in turn.
        assert!(
            started.elapsed() >= Duration::from_millis(160),
            "native ring: {native_ring}: {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn every_connection_is_served_in_its_own_context_on_both_routes() {
    const PEERS: u8 = 12;
    let llvm = compile_program("tcp_contexts.wf");
    assert!(
        llvm.contains("@wf__context_launch("),
        "the accept loop starts contexts"
    );
    let program = build_program(&llvm);
    let routes: &[bool] = if cfg!(windows) {
        &[true]
    } else {
        &[true, false]
    };
    for &native_ring in routes {
        let port = free_port();
        let text = port.to_string();
        let count = PEERS.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), count.as_bytes()]);
        let mut streams = (0..PEERS)
            .map(|_| connect_when_ready(port))
            .collect::<Vec<_>>();
        for peer in (0..PEERS).rev() {
            let stream = &mut streams[usize::from(peer)];
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .expect("bound the wait for this peer's answer");
            let sent = [peer, peer + 1, peer + 2];
            stream.write_all(&sent).expect("send this peer's bytes");
            stream
                .shutdown(std::net::Shutdown::Write)
                .expect("finish this peer's sending");
            let mut returned = Vec::new();
            stream.read_to_end(&mut returned).unwrap_or_else(|error| {
                panic!(
                    "peer {peer} was not answered while earlier peers were silent \
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

/// Contexts run on several driver threads where the host has a ring, each
/// driver with a ring of its own: with four drivers pinned, 64 peers that all
/// speak at once are each answered, and the server exits zero once every one
/// has closed, which it does only when every context, on whichever driver it
/// ran, has finished before the entry leaves. The last context's finish and
/// the entry's exit happen on different threads, so the case runs several
/// rounds. A host whose kernel refuses the ring runs one driver, and the case
/// then checks only the answers and the exit.
#[cfg(target_os = "linux")]
#[test]
fn contexts_on_four_drivers_serve_every_peer_and_finish_before_the_entry() {
    const PEERS: usize = 64;
    let program = build_program(&compile_program("tcp_contexts.wf"));
    for round in 0..6 {
        let port = free_port();
        let text = port.to_string();
        let count = PEERS.to_string();
        let child = program.spawn_on_route_with(
            true,
            &[("WF_DRIVERS", "4")],
            &[text.as_bytes(), count.as_bytes()],
        );
        let mut streams = (0..PEERS)
            .map(|_| connect_when_ready(port))
            .collect::<Vec<_>>();
        for (peer, stream) in streams.iter_mut().enumerate() {
            let sent = [round as u8, peer as u8, 7];
            stream.write_all(&sent).expect("send this peer's bytes");
        }
        for (peer, stream) in streams.iter_mut().enumerate() {
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .expect("bound the wait for this peer's answer");
            let mut returned = [0_u8; 3];
            stream
                .read_exact(&mut returned)
                .unwrap_or_else(|error| panic!("peer {peer} of round {round}: {error}"));
            assert_eq!(
                returned,
                [round as u8, peer as u8, 7],
                "peer {peer} of round {round}"
            );
        }
        let rings = std::fs::read_dir(format!("/proc/{}/fd", child.id()))
            .expect("list the server's descriptors")
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .filter(|target| target.to_string_lossy().contains("io_uring"))
            .count();
        assert!(
            rings == 0 || rings == 4,
            "a server that has a ring runs four drivers with one ring each, \
             and this one holds {rings} rings (round {round})"
        );
        drop(streams);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "round {round}");
    }
}

/// Two bound spawned fetches proceed together [WAIT-3]: the first server
/// answers only once the second has received its request, which a program
/// that waited for the first fetch's byte before sending the second request
/// never sends. Each result is joined where it is first used, the sum.
#[test]
fn two_bound_fetches_proceed_together_on_both_routes() {
    let llvm = compile_program("tcp_gather.wf");
    assert!(
        llvm.contains("@wf__context_launch("),
        "each spawned fetch starts a context"
    );
    let program = build_program(&llvm);
    for native_ring in [true, false] {
        let first = TcpListener::bind("127.0.0.1:0").expect("the first server's port");
        let second = TcpListener::bind("127.0.0.1:0").expect("the second server's port");
        let first_port = first
            .local_addr()
            .expect("first address")
            .port()
            .to_string();
        let second_port = second
            .local_addr()
            .expect("second address")
            .port()
            .to_string();
        let (arrived, second_request) = std::sync::mpsc::channel();
        let second_server = std::thread::spawn(move || {
            let mut stream = accept_when_ready(&second);
            let mut request = [0_u8; 1];
            stream.read_exact(&mut request).expect("the second request");
            arrived.send(request[0]).expect("tell the first server");
            stream.write_all(&[20]).expect("the second answer");
        });
        let first_server = std::thread::spawn(move || {
            let mut stream = accept_when_ready(&first);
            let mut request = [0_u8; 1];
            stream.read_exact(&mut request).expect("the first request");
            let other = second_request
                .recv_timeout(Duration::from_secs(20))
                .expect("the second request arrived while the first fetch waited");
            stream.write_all(&[10]).expect("the first answer");
            (request[0], other)
        });
        let child = program.spawn_on_route(
            native_ring,
            &[first_port.as_bytes(), second_port.as_bytes()],
        );
        let (status, _) = finished(child);
        assert_eq!(
            first_server.join().expect("the first server"),
            (1, 2),
            "native ring: {native_ring}"
        );
        second_server.join().expect("the second server");
        assert_eq!(status, 30, "native ring: {native_ring}");
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

fn close_pair(factory: &std::io::HandleFactory, connection: std::net::TcpConnection, receive_first: Bool) -> result: u8 writes(factory) waits {
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

fn remaining(connection: &std::net::TcpConnection) -> result: u8 writes(connection.receive), writes(connection.send) waits {
  let bytes = slots_new::<u8, 1>();
  place_back(window: &bytes, value: 0_u8);
  let destination = &bytes[0_u64..1_u64];
  let no_deadline = None<std::time::Instant>();
  match std::net::receive_next(receive: &connection^.receive, destination: destination, start: 0_u64, end: 1_u64, deadline: no_deadline) {
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
  match std::net::send_once(send: &connection^.send, source: source, start: 0_u64, end: 1_u64, deadline: no_deadline) {
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

fn exercise(factory: &std::io::HandleFactory, address: &std::net::SocketAddress) -> result: u8 reads(address), writes(factory) waits {
  let receive_first = True();
  let send_first = False();
  let no_deadline = None<std::time::Instant>();
  match std::net::tcp_connect(factory: factory, address: address, deadline: no_deadline) {
    Ok(value: first) => {
      match std::net::tcp_connect(factory: factory, address: address, deadline: no_deadline) {
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
          match std::net::tcp_connect(factory: factory, address: address, deadline: no_deadline) {
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

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(args: args, cwd: cwd_directory, stdout: out, stderr: err, handles: handles, stdin: input, clock: unused_clock, wall_clock: unused_wall_clock) = move inputs;
  let std::fs::Directory(read: cwd, write: cwd_write) = move cwd_directory;
  std::fs::close_directory_write(factory: &handles, directory: move cwd_write);
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

/// One RESP2 request of bulk strings.
#[cfg(target_os = "linux")]
fn resp(arguments: &[&str]) -> Vec<u8> {
    let mut bytes = format!("*{}\r\n", arguments.len()).into_bytes();
    for argument in arguments {
        bytes.extend_from_slice(format!("${}\r\n{argument}\r\n", argument.len()).as_bytes());
    }
    bytes
}

/// firn, the Redis-compatible server in `apps/firn`, built once for every
/// case that runs it; each case names its own append-only file, so the shared
/// working directory holds no state one case leaves for another.
#[cfg(target_os = "linux")]
fn firn() -> &'static CompiledProgram {
    static PROGRAM: std::sync::OnceLock<CompiledProgram> = std::sync::OnceLock::new();
    PROGRAM.get_or_init(|| build_program(&compile_app("firn", "firn")))
}

/// Reads one reply line through its CR LF.
#[cfg(target_os = "linux")]
fn reply_line(stream: &mut TcpStream, what: &str) -> String {
    let mut byte = [0_u8; 1];
    let mut line = Vec::new();
    while !line.ends_with(b"\r\n") {
        stream
            .read_exact(&mut byte)
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        line.push(byte[0]);
    }
    String::from_utf8_lossy(&line).into_owned()
}

/// Reads one RESP integer reply.
#[cfg(target_os = "linux")]
fn integer_reply(stream: &mut TcpStream, what: &str) -> i64 {
    let line = reply_line(stream, what);
    line.strip_prefix(':')
        .and_then(|rest| rest.strip_suffix("\r\n"))
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_else(|| panic!("{what}: not an integer reply: {line:?}"))
}

/// Reads exactly the bytes of the expected replies and compares them.
#[cfg(target_os = "linux")]
fn expect_replies(stream: &mut TcpStream, expected: &[u8], what: &str) {
    let mut returned = vec![0_u8; expected.len()];
    stream
        .read_exact(&mut returned)
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    assert_eq!(
        String::from_utf8_lossy(&returned),
        String::from_utf8_lossy(expected),
        "{what}"
    );
}

/// [SHARE-1, SHARE-3] firn serves every client over one
/// keyspace: the commands answer as Redis does, a pipelined batch and a
/// command split across two sends are answered whole, and clients that
/// increment one key from four drivers at once lose no increment, which a
/// keyspace not held alone by each atomic statement would.
#[cfg(target_os = "linux")]
#[test]
fn firn_serves_every_client_over_one_keyspace() {
    const CLIENTS: usize = 8;
    const INCREMENTS: usize = 250;
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let count = (CLIENTS + 1).to_string();
    let child = program.spawn_on_route_with(
        true,
        &[("WF_DRIVERS", "4")],
        &[text.as_bytes(), count.as_bytes()],
    );
    let mut first = connect_when_ready(port);
    first
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the first client's waits");
    let mut batch = Vec::new();
    for request in [
        vec!["PING"],
        vec!["SET", "fruit", "apple"],
        vec!["GET", "fruit"],
        vec!["GET", "absent"],
        vec!["INCR", "fruit"],
        vec!["DEL", "fruit"],
        vec!["DEL", "fruit"],
        vec!["INCR", "total"],
        vec!["CONFIG", "GET", "save"],
    ] {
        batch.extend(resp(&request));
    }
    first.write_all(&batch).expect("send a pipelined batch");
    expect_replies(
        &mut first,
        b"+PONG\r\n+OK\r\n$5\r\napple\r\n$-1\r\n-ERR value is not an integer or out of range\r\n:1\r\n:0\r\n:1\r\n*2\r\n$4\r\nsave\r\n$0\r\n\r\n",
        "the pipelined batch",
    );
    let split = resp(&["SET", "split", "across two sends"]);
    let (head, tail) = split.split_at(11);
    first.write_all(head).expect("send the first part");
    first.flush().expect("flush the first part");
    std::thread::sleep(Duration::from_millis(50));
    first.write_all(tail).expect("send the rest");
    first
        .write_all(&resp(&["GET", "split"]))
        .expect("read it back");
    expect_replies(
        &mut first,
        b"+OK\r\n$16\r\nacross two sends\r\n",
        "a command split across two sends",
    );
    let clients = (0..CLIENTS)
        .map(|client| {
            let mut stream = connect_when_ready(port);
            std::thread::spawn(move || {
                stream
                    .set_read_timeout(Some(Duration::from_secs(20)))
                    .expect("bound this client's waits");
                for _ in 0..INCREMENTS {
                    stream
                        .write_all(&resp(&["INCR", "hits"]))
                        .expect("send an increment");
                    let mut reply = [0_u8; 1];
                    let mut line = Vec::new();
                    loop {
                        stream
                            .read_exact(&mut reply)
                            .unwrap_or_else(|error| panic!("client {client}: {error}"));
                        line.push(reply[0]);
                        if line.ends_with(b"\r\n") {
                            break;
                        }
                    }
                    assert_eq!(line[0], b':', "client {client}: {line:?}");
                }
            })
        })
        .collect::<Vec<_>>();
    for client in clients {
        client.join().expect("a client finished");
    }
    let total = CLIENTS * INCREMENTS;
    first
        .write_all(&resp(&["GET", "hits"]))
        .expect("read the counter");
    expect_replies(
        &mut first,
        format!("${}\r\n{total}\r\n", total.to_string().len()).as_bytes(),
        "every increment",
    );
    drop(first);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// [PRE-2] firn expires keys as Redis does. In one pipelined
/// batch, whose commands all see one reading of the clocks, `TTL` answers -1
/// for a key without an expiry and -2 for an absent one, `EXPIRE` gives one,
/// `PERSIST` removes it once, and `SET` refuses a zero expiry, an unknown
/// option and a count that is not a number. A key set with `PX 100` answers a
/// positive `PTTL` and is absent 200 milliseconds later. A key read 5
/// milliseconds after its expiry is absent too, which the command's own check
/// answers: the expiring context wakes only every 100 milliseconds, so without
/// that check the value would usually still be returned.
#[cfg(target_os = "linux")]
#[test]
fn firn_expires_keys_on_both_routes() {
    let program = firn();
    for native_ring in [true, false] {
        let what = format!("native ring: {native_ring}");
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1"]);
        let mut client = connect_when_ready(port);
        client
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("bound the client's waits");
        let mut batch = Vec::new();
        for request in [
            vec!["SET", "kept", "1"],
            vec!["SET", "brief", "hello", "PX", "100"],
            vec!["TTL", "kept"],
            vec!["TTL", "absent"],
            vec!["EXPIRE", "kept", "100"],
            vec!["TTL", "kept"],
            vec!["PERSIST", "kept"],
            vec!["TTL", "kept"],
            vec!["PERSIST", "kept"],
            vec!["SET", "other", "v", "EX", "0"],
            vec!["SET", "other", "v", "XX", "1"],
            vec!["SET", "other", "v", "PX", "soon"],
            vec!["DBSIZE"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("send the expiry batch");
        expect_replies(
            &mut client,
            b"+OK\r\n+OK\r\n:-1\r\n:-2\r\n:1\r\n:100\r\n:1\r\n:-1\r\n:0\r\n-ERR invalid expire time in 'set' command\r\n-ERR syntax error\r\n-ERR value is not an integer or out of range\r\n:2\r\n",
            &what,
        );
        client
            .write_all(&resp(&["PTTL", "brief"]))
            .expect("ask the time left");
        let left = integer_reply(&mut client, &what);
        assert!((1..=100).contains(&left), "{what}: {left}");
        std::thread::sleep(Duration::from_millis(200));
        let mut after = resp(&["GET", "brief"]);
        after.extend(resp(&["PTTL", "brief"]));
        after.extend(resp(&["DBSIZE"]));
        client.write_all(&after).expect("read the expired key");
        expect_replies(&mut client, b"$-1\r\n:-2\r\n:1\r\n", &what);
        client
            .write_all(&resp(&["SET", "flash", "v", "PX", "1"]))
            .expect("set a key that expires at once");
        expect_replies(&mut client, b"+OK\r\n", &what);
        std::thread::sleep(Duration::from_millis(5));
        client
            .write_all(&resp(&["GET", "flash"]))
            .expect("read it after its expiry");
        expect_replies(&mut client, b"$-1\r\n", &what);
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}");
    }
}

/// [PRE-2] firn's expiring context removes keys no command reads:
/// a thousand keys set with `PX 50` leave `DBSIZE`, which reads no key, at
/// zero within three seconds.
#[cfg(target_os = "linux")]
#[test]
fn firn_removes_expired_keys_no_command_reads_on_both_routes() {
    const KEYS: usize = 1000;
    let program = firn();
    for native_ring in [true, false] {
        let what = format!("native ring: {native_ring}");
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1"]);
        let mut client = connect_when_ready(port);
        client
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("bound the client's waits");
        let mut batch = Vec::new();
        for index in 0..KEYS {
            let key = format!("key:{index}");
            batch.extend(resp(&["SET", &key, "v", "PX", "50"]));
        }
        client.write_all(&batch).expect("send the keys");
        expect_replies(&mut client, &b"+OK\r\n".repeat(KEYS), &what);
        let started = Instant::now();
        loop {
            client
                .write_all(&resp(&["DBSIZE"]))
                .expect("count the keys");
            let size = integer_reply(&mut client, &what);
            if size == 0 {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "{what}: {size} keys left"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}");
    }
}

/// [PRE-2] firn replays its append-only file after a restart:
/// every change the first run made, set, removed, incremented, given an expiry
/// or made persistent, holds in the second, and a key whose expiry passed
/// while firn was stopped is absent. As in Redis, the replay applies the
/// file's commands in order without expiring anything, so a key made
/// persistent before its expiry holds its value, and a key incremented before
/// its expiry passed is absent rather than counting again from one. The first run ends once its one
/// client has closed, after its writer appended and synced the last changes.
#[cfg(target_os = "linux")]
#[test]
fn firn_replays_its_append_only_file_after_a_restart_on_both_routes() {
    let program = firn();
    for native_ring in [true, false] {
        let what = format!("native ring: {native_ring}");
        let name = format!("replay-{native_ring}.aof");
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1", name.as_bytes()]);
        let mut client = connect_when_ready(port);
        client
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("bound the first client's waits");
        let mut batch = Vec::new();
        for request in [
            vec!["SET", "gone", "v"],
            vec!["SET", "brief", "v", "PX", "300"],
            vec!["SET", "long", "v", "EX", "100"],
            vec!["INCR", "count"],
            vec!["INCR", "count"],
            vec!["DEL", "gone"],
            vec!["SET", "kept", "v", "PX", "60000"],
            vec!["PERSIST", "kept"],
            vec!["SET", "later", "v", "PX", "60000"],
            vec!["SET", "persisted", "v", "PX", "300"],
            vec!["PERSIST", "persisted"],
            vec!["SET", "bumped", "5", "PX", "300"],
            vec!["INCR", "bumped"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("send the changes");
        expect_replies(
            &mut client,
            b"+OK\r\n+OK\r\n+OK\r\n:1\r\n:2\r\n:1\r\n+OK\r\n:1\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n:6\r\n",
            &what,
        );
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}: the first run");
        std::thread::sleep(Duration::from_millis(400));
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1", name.as_bytes()]);
        let mut client = connect_when_ready(port);
        client
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("bound the second client's waits");
        let mut batch = Vec::new();
        for request in [
            vec!["GET", "gone"],
            vec!["GET", "brief"],
            vec!["GET", "long"],
            vec!["GET", "count"],
            vec!["GET", "kept"],
            vec!["TTL", "kept"],
            vec!["GET", "persisted"],
            vec!["TTL", "persisted"],
            vec!["GET", "bumped"],
            vec!["DBSIZE"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("read the replayed keys");
        expect_replies(
            &mut client,
            b"$-1\r\n$-1\r\n$1\r\nv\r\n$1\r\n2\r\n$1\r\nv\r\n:-1\r\n$1\r\nv\r\n:-1\r\n$-1\r\n:5\r\n",
            &what,
        );
        client
            .write_all(&resp(&["TTL", "long"]))
            .expect("ask the time left");
        let long = integer_reply(&mut client, &what);
        assert!((98..=100).contains(&long), "{what}: {long}");
        client
            .write_all(&resp(&["PTTL", "later"]))
            .expect("ask the time left");
        let later = integer_reply(&mut client, &what);
        assert!((58_000..=60_000).contains(&later), "{what}: {later}");
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}: the second run");
    }
}

/// [PRE-2] a deadline on `receive_next` closes a client silent past
/// firn's idle limit: with a limit of one second, the connection ends after
/// at least 0.9 and at most two seconds of silence.
#[cfg(target_os = "linux")]
#[test]
fn firn_closes_a_client_silent_past_its_idle_limit_on_both_routes() {
    let program = firn();
    for native_ring in [true, false] {
        let what = format!("native ring: {native_ring}");
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1", b"-", b"1"]);
        let mut client = connect_when_ready(port);
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("bound the client's waits");
        client.write_all(&resp(&["PING"])).expect("send a ping");
        expect_replies(&mut client, b"+PONG\r\n", &what);
        let started = Instant::now();
        let mut rest = [0_u8; 16];
        let read = client
            .read(&mut rest)
            .unwrap_or_else(|error| panic!("{what}: the connection stayed open: {error}"));
        let silent = started.elapsed();
        assert_eq!(read, 0, "{what}: {:?}", &rest[..read]);
        assert!(
            silent >= Duration::from_millis(900) && silent <= Duration::from_secs(2),
            "{what}: closed after {silent:?}"
        );
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}");
    }
}

/// firn answers the value types as Redis does. One pipelined batch pushes,
/// ranges and pops a list until it is empty, which removes its key; adds and
/// removes set members; sets and reads hash fields; adds, rescores and pops
/// sorted-set members; refuses a list command on a string and a string command
/// on a hash; sets ten keys in one MSET, a command of eleven arguments; and
/// names an unknown command and a command short of arguments as Redis does.
/// Two inline commands close the batch. The expected bytes are those
/// redis-server 7.0.15 returns for the same bytes.
#[cfg(target_os = "linux")]
#[test]
fn firn_answers_the_value_types_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1"]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the client's waits");
    let mut batch = Vec::new();
    for request in [
        vec!["RPUSH", "l", "a", "b", "c"],
        vec!["LPUSH", "l", "z"],
        vec!["LRANGE", "l", "0", "-1"],
        vec!["LRANGE", "l", "1", "-2"],
        vec!["LPOP", "l"],
        vec!["RPOP", "l", "2"],
        vec!["LLEN", "l"],
        vec!["RPOP", "l"],
        vec!["EXISTS", "l"],
        vec!["SADD", "s", "a", "b", "c", "a"],
        vec!["SREM", "s", "a", "q"],
        vec!["SCARD", "s"],
        vec!["HSET", "h", "f", "1", "g", "2"],
        vec!["HSET", "h", "f", "3"],
        vec!["HGET", "h", "f"],
        vec!["HGET", "h", "nope"],
        vec!["ZADD", "z", "3", "c", "1", "a", "2", "b"],
        vec!["ZADD", "z", "0", "c"],
        vec!["ZSCORE", "z", "c"],
        vec!["ZPOPMIN", "z", "2"],
        vec!["ZCARD", "z"],
        vec!["SET", "str", "v"],
        vec!["LPUSH", "str", "x"],
        vec!["SADD", "l", "x"],
        vec!["TYPE", "l"],
        vec!["GET", "h"],
        vec![
            "MSET", "k1", "1", "k2", "2", "k3", "3", "k4", "4", "k5", "5",
        ],
        vec!["GET", "k5"],
        vec!["NOPE", "a", "b"],
        vec!["LLEN"],
    ] {
        batch.extend(resp(&request));
    }
    batch.extend_from_slice(b"SET inline yes\r\nGET inline\r\n");
    client.write_all(&batch).expect("send the batch");
    expect_replies(
        &mut client,
        b":3\r\n:4\r\n*4\r\n$1\r\nz\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nc\r\n*2\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nz\r\n*2\r\n$1\r\nc\r\n$1\r\nb\r\n:1\r\n$1\r\na\r\n:0\r\n:3\r\n:1\r\n:2\r\n:2\r\n:0\r\n$1\r\n3\r\n$-1\r\n:3\r\n:0\r\n$1\r\n0\r\n*4\r\n$1\r\nc\r\n$1\r\n0\r\n$1\r\na\r\n$1\r\n1\r\n:1\r\n+OK\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n:1\r\n+set\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n+OK\r\n$1\r\n5\r\n-ERR unknown command 'NOPE', with args beginning with: 'a' 'b' \r\n-ERR wrong number of arguments for 'llen' command\r\n+OK\r\n$3\r\nyes\r\n",
        "the value-type batch",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn keeps strings of at most 24 bytes inside the keyspace and longer ones
/// in allocations of their own. Keys, members, fields and values of exactly 24
/// bytes and of 25 bytes sharing those 24, and a key differing from them only
/// in its last byte, stay distinct in every kind of value, and a number that
/// `INCR` rewrites to a different length reads back whole; the expected replies
/// are redis-server 7.0.15's to the same requests.
#[cfg(target_os = "linux")]
#[test]
fn firn_keeps_strings_at_and_past_the_inline_length_apart() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1"]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the client's waits");
    let at = format!("{}x", "a".repeat(23));
    let past = format!("{at}y");
    let other = format!("{}z", "a".repeat(23));
    let (a, b, c) = (at.as_str(), past.as_str(), other.as_str());
    let short_value = "v".repeat(24);
    let long_value = "v".repeat(25);
    let (v24, v25) = (short_value.as_str(), long_value.as_str());
    let wide = format!("1{}", "0".repeat(25));
    let mut batch = Vec::new();
    for request in [
        vec!["SET", a, "1"],
        vec!["SET", b, "2"],
        vec!["SET", c, "3"],
        vec!["GET", a],
        vec!["GET", b],
        vec!["GET", c],
        vec!["INCR", a],
        vec!["INCR", b],
        vec!["GET", a],
        vec!["GET", b],
        vec!["SET", "k", v24],
        vec!["GET", "k"],
        vec!["SET", "k", v25],
        vec!["GET", "k"],
        vec!["SET", "k", v24],
        vec!["GET", "k"],
        vec!["SET", "m", "9223372036854775806"],
        vec!["INCR", "m"],
        vec!["INCR", "m"],
        vec!["GET", "m"],
        vec!["SET", "neg", "-9223372036854775807"],
        vec!["INCR", "neg"],
        vec!["GET", "neg"],
        vec!["SET", "wide", wide.as_str()],
        vec!["INCR", "wide"],
        vec!["SADD", "s", a, b, c, a],
        vec!["SCARD", "s"],
        vec!["SREM", "s", a],
        vec!["SCARD", "s"],
        vec!["SREM", "s", a, b],
        vec!["SCARD", "s"],
        vec!["HSET", "h", a, v25, b, v24],
        vec!["HGET", "h", a],
        vec!["HGET", "h", b],
        vec!["HGET", "h", c],
        vec!["ZADD", "z", "1", a, "2", b, "3", c],
        vec!["ZSCORE", "z", b],
        vec!["ZADD", "z", "0", b],
        vec!["ZPOPMIN", "z", "2"],
        vec!["LPUSH", "l", a, b],
        vec!["LRANGE", "l", "0", "-1"],
        vec!["EXISTS", a, b, c],
        vec!["DEL", a, b],
        vec!["EXISTS", a, b, c],
        vec!["MSET", a, v25, b, v24],
        vec!["GET", a],
        vec!["GET", b],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the batch");
    expect_replies(
        &mut client,
        b"+OK\r\n+OK\r\n+OK\r\n$1\r\n1\r\n$1\r\n2\r\n$1\r\n3\r\n:2\r\n:3\r\n$1\r\n2\r\n$1\r\n3\r\n+OK\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n:9223372036854775807\r\n-ERR increment or decrement would overflow\r\n$19\r\n9223372036854775807\r\n+OK\r\n:-9223372036854775806\r\n$20\r\n-9223372036854775806\r\n+OK\r\n-ERR value is not an integer or out of range\r\n:3\r\n:3\r\n:1\r\n:2\r\n:1\r\n:1\r\n:2\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n$-1\r\n:3\r\n$1\r\n2\r\n:0\r\n*4\r\n$25\r\naaaaaaaaaaaaaaaaaaaaaaaxy\r\n$1\r\n0\r\n$24\r\naaaaaaaaaaaaaaaaaaaaaaax\r\n$1\r\n1\r\n:2\r\n*2\r\n$25\r\naaaaaaaaaaaaaaaaaaaaaaaxy\r\n$24\r\naaaaaaaaaaaaaaaaaaaaaaax\r\n:3\r\n:2\r\n:1\r\n+OK\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n",
        "the strings around the inline length",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn reads a request larger than its first input window and writes a reply
/// larger than its first reply window: a 100,000-byte value set in one command
/// reads back whole, and a range of 2,000 list elements, a reply of more than
/// 16,384 bytes, arrives whole and in order.
#[cfg(target_os = "linux")]
#[test]
fn firn_carries_requests_and_replies_larger_than_its_windows() {
    const ELEMENTS: usize = 2_000;
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1"]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the client's waits");
    let value = (0..100_000)
        .map(|index| char::from(b'a' + (index % 26) as u8))
        .collect::<String>();
    client
        .write_all(&resp(&["SET", "big", &value]))
        .expect("send a large value");
    client
        .write_all(&resp(&["GET", "big"]))
        .expect("read it back");
    let mut expected = b"+OK\r\n".to_vec();
    expected.extend_from_slice(format!("${}\r\n{value}\r\n", value.len()).as_bytes());
    expect_replies(&mut client, &expected, "the large value");
    let elements = (0..ELEMENTS)
        .map(|index| format!("element-{index:05}"))
        .collect::<Vec<_>>();
    let mut push = vec!["RPUSH", "many"];
    push.extend(elements.iter().map(String::as_str));
    client.write_all(&resp(&push)).expect("push the elements");
    client
        .write_all(&resp(&["LRANGE", "many", "0", "-1"]))
        .expect("range every element");
    let mut expected = format!(":{ELEMENTS}\r\n*{ELEMENTS}\r\n").into_bytes();
    for element in &elements {
        expected.extend_from_slice(format!("${}\r\n{element}\r\n", element.len()).as_bytes());
    }
    assert!(expected.len() > 16_384);
    expect_replies(&mut client, &expected, "the large range");
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn replays the value types from its append-only file: after a restart a
/// list keeps the elements its pushes and pops left, a hash its field, and a
/// sorted set the member ZPOPMIN left at its score. The 25 of 50 members SPOP
/// removed stay removed, since the file records the pop as the SREM of the
/// members it chose, as Redis records it; a replay that popped at random, from
/// a generator seeded by the clock at each start, would almost surely remove
/// others.
#[cfg(target_os = "linux")]
#[test]
fn firn_replays_the_value_types_from_its_append_only_file() {
    let program = firn();
    let name = "replay-types.aof";
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1", name.as_bytes()]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the first client's waits");
    let members = (0..50)
        .map(|index| format!("m{index:02}"))
        .collect::<Vec<_>>();
    let mut add = vec!["SADD", "s"];
    add.extend(members.iter().map(String::as_str));
    let mut batch = Vec::new();
    for request in [
        vec!["RPUSH", "l", "a", "b", "c"],
        vec!["LPOP", "l"],
        vec!["HSET", "h", "f", "v"],
        vec!["ZADD", "z", "1", "a", "2", "b"],
        vec!["ZPOPMIN", "z"],
        add,
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the changes");
    expect_replies(
        &mut client,
        b":3\r\n$1\r\na\r\n:1\r\n:2\r\n*2\r\n$1\r\na\r\n$1\r\n1\r\n:50\r\n",
        "the first run's changes",
    );
    client
        .write_all(&resp(&["SPOP", "s", "25"]))
        .expect("pop 25 members");
    assert_eq!(
        reply_line(&mut client, "the popped members' header"),
        "*25\r\n"
    );
    let mut popped = Vec::new();
    for _ in 0..25 {
        assert_eq!(
            reply_line(&mut client, "a popped member's header"),
            "$3\r\n"
        );
        let member = reply_line(&mut client, "a popped member");
        let member = member.trim_end().to_owned();
        assert!(members.contains(&member), "{member}");
        assert!(!popped.contains(&member), "{member} popped twice");
        popped.push(member);
    }
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0, "the first run");
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1", name.as_bytes()]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the second client's waits");
    let mut remove = vec!["SREM", "s"];
    remove.extend(popped.iter().map(String::as_str));
    let mut batch = Vec::new();
    for request in [
        vec!["LRANGE", "l", "0", "-1"],
        vec!["HGET", "h", "f"],
        vec!["ZCARD", "z"],
        vec!["ZSCORE", "z", "b"],
        vec!["SCARD", "s"],
        remove,
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("read the replayed values");
    expect_replies(
        &mut client,
        b"*2\r\n$1\r\nb\r\n$1\r\nc\r\n$1\r\nv\r\n:1\r\n$1\r\n2\r\n:25\r\n:0\r\n",
        "the replayed values",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0, "the second run");
}
