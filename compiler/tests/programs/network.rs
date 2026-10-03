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
/// so the port is free the moment this drops, and the program's own `bind`
/// would take it even on a host where the runtime set no `SO_REUSEADDR`.
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
    connect_to_when_ready(SocketAddr::from(([127, 0, 0, 1], port)))
}

/// Connects to a program that is still starting, at the address it listens on.
fn connect_to_when_ready(address: SocketAddr) -> TcpStream {
    let port = address.port();
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

/// Reads one `TIME` reply as calendar milliseconds.
#[cfg(target_os = "linux")]
fn time_reply(stream: &mut TcpStream, what: &str) -> u64 {
    assert_eq!(reply_line(stream, what), "*2\r\n", "{what}: TIME");
    let mut parts = [0_u64; 2];
    for part in &mut parts {
        reply_line(stream, what);
        let digits = reply_line(stream, what);
        *part = digits
            .trim_end()
            .parse()
            .unwrap_or_else(|_| panic!("{what}: TIME answered {digits:?}"));
    }
    parts[0] * 1000 + parts[1] / 1000
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
/// option and a count that is not a number. An expiry of 10^11 seconds, past
/// what nanoseconds in 64 bits hold, is kept to the second, as Redis keeps
/// expiries in calendar milliseconds; a negative `EXPIRE` removes the key at
/// once, so that `DBSIZE` no longer counts it, and
/// one whose milliseconds leave the range of i64, alone or added to the time,
/// is refused, also when they would wrap to a small number. `EXPIRE` takes NX,
/// XX, GT and LT as Redis 7.0.15 does, a key without an expiry failing GT and
/// passing LT, refuses NX beside another option and GT beside LT, and echoes
/// an unknown option up to a zero byte, its trailing line ends dropped and any
/// others written as spaces; `EXPIREAT` and `PEXPIREAT` set calendar times
/// that `EXPIRETIME`, rounded to the second as Redis rounds it, and
/// `PEXPIRETIME` give back exactly, and a name only sharing their first eight
/// letters is unknown. A key set with `PX 100` answers a
/// positive `PTTL` and is absent 200 milliseconds later. A key read 5
/// milliseconds after its expiry is absent too, which the command's own check
/// answers: the expiring context wakes only every 100 milliseconds, so without
/// that check the value would usually still be returned. A key lives through
/// the millisecond its expiry names, as Redis's `keyIsExpired` keeps it: of
/// keys set to expire at each of a hundred milliseconds from a reading of the
/// clock, each read back at once in a batch whose commands share the reading a
/// `TIME` before and after them reports, those expiring at that reading or
/// later answer their value and the earlier ones nil.
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
            vec!["SET", "far", "v", "EX", "100000000000"],
            vec!["TTL", "far"],
            vec!["EXPIRE", "far", "-1"],
            vec!["DBSIZE"],
            vec!["EXPIRE", "absent", "-1"],
            vec!["SET", "other", "v", "EX", "9223372036854776"],
            vec!["SET", "other", "v", "PX", "9223372036854775807"],
            vec!["SET", "other", "v", "EXAT", "9223372036854776"],
            vec!["PEXPIRE", "kept", "9223372036854775000"],
            vec!["EXPIRE", "kept", "18446744073709552"],
            vec!["EXPIRE", "kept", "-18446744073709552"],
            vec!["SET", "e", "v"],
            vec!["EXPIRE", "e", "100", "nx"],
            vec!["EXPIRE", "e", "200", "NX"],
            vec!["EXPIRE", "e", "50", "gt"],
            vec!["EXPIRE", "e", "300", "GT"],
            vec!["TTL", "e"],
            vec!["EXPIRE", "e", "400", "lt"],
            vec!["EXPIRE", "e", "100", "LT"],
            vec!["EXPIRE", "e", "100", "XX"],
            vec!["PERSIST", "e"],
            vec!["EXPIRE", "e", "100", "XX"],
            vec!["EXPIRE", "e", "100", "GT"],
            vec!["EXPIRE", "e", "100", "LT"],
            vec!["EXPIRE", "e", "100", "NX", "GT"],
            vec!["EXPIRE", "e", "100", "GT", "LT"],
            vec!["EXPIRE", "e", "abc", "FOO"],
            vec!["EXPIRE", "e", "100", "\r\nab\nc\r"],
            vec!["EXPIRE", "e", "100", "fo\0o"],
            vec!["EXPIRE", "e", "100", "nx\0garbage"],
            vec!["EXPIREAT", "e", "99999999999"],
            vec!["EXPIRETIME", "e"],
            vec!["PEXPIRETIME", "e"],
            vec!["PEXPIREAT", "e", "99999999999500"],
            vec!["EXPIRETIME", "e"],
            vec!["PEXPIREAT", "e", "99999999999499"],
            vec!["EXPIRETIME", "e"],
            vec!["PEXPIREAT", "e", "99999999999499", "GT"],
            vec!["PEXPIREAT", "e", "99999999999499", "LT"],
            vec!["EXPIRE", "e", "100", "ab\r\n\0x"],
            vec!["EXPIREAT", "e", "9223372036854776"],
            vec!["EXPIRETIME", "kept"],
            vec!["PEXPIRETIME", "absent"],
            vec!["EXPIRE", "e", "-1", "GT"],
            vec!["EXPIRE", "e", "-1", "XX"],
            vec!["EXISTS", "e"],
            vec!["PEXPIRETIMEX", "e"],
            vec!["EXPIRETIMEX", "e"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("send the expiry batch");
        expect_replies(
            &mut client,
            b"+OK\r\n+OK\r\n:-1\r\n:-2\r\n:1\r\n:100\r\n:1\r\n:-1\r\n:0\r\n-ERR invalid expire time in 'set' command\r\n-ERR syntax error\r\n-ERR value is not an integer or out of range\r\n:2\r\n+OK\r\n:100000000000\r\n:1\r\n:2\r\n:0\r\n-ERR invalid expire time in 'set' command\r\n-ERR invalid expire time in 'set' command\r\n-ERR invalid expire time in 'set' command\r\n-ERR invalid expire time in 'pexpire' command\r\n-ERR invalid expire time in 'expire' command\r\n-ERR invalid expire time in 'expire' command\r\n+OK\r\n:1\r\n:0\r\n:0\r\n:1\r\n:300\r\n:0\r\n:1\r\n:1\r\n:1\r\n:0\r\n:0\r\n:1\r\n-ERR NX and XX, GT or LT options at the same time are not compatible\r\n-ERR GT and LT options at the same time are not compatible\r\n-ERR Unsupported option FOO\r\n-ERR Unsupported option   ab c\r\n-ERR Unsupported option fo\r\n:0\r\n:1\r\n:99999999999\r\n:99999999999000\r\n:1\r\n:100000000000\r\n:1\r\n:99999999999\r\n:0\r\n:0\r\n-ERR Unsupported option ab\r\n-ERR invalid expire time in 'expireat' command\r\n:-1\r\n:-2\r\n:0\r\n:1\r\n:0\r\n-ERR unknown command 'PEXPIRETIMEX', with args beginning with: 'e' \r\n-ERR unknown command 'EXPIRETIMEX', with args beginning with: 'e' \r\n",
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
        // A batch the server read in two parts, or later than the hundredth
        // millisecond, cannot place the boundary and is sent again.
        let mut placed = false;
        for _ in 0..10 {
            client.write_all(&resp(&["TIME"])).expect("ask the time");
            let start = time_reply(&mut client, &what);
            let mut edge = resp(&["TIME"]);
            for offset in 0..100_u64 {
                let key = format!("edge:{offset}");
                let at = (start + offset).to_string();
                edge.extend(resp(&["SET", &key, "v", "PXAT", &at]));
                edge.extend(resp(&["GET", &key]));
            }
            edge.extend(resp(&["TIME"]));
            client
                .write_all(&edge)
                .expect("set keys expiring around the time");
            let now = time_reply(&mut client, &what);
            let mut alive = Vec::new();
            for _ in 0..100 {
                assert_eq!(reply_line(&mut client, &what), "+OK\r\n", "{what}");
                let header = reply_line(&mut client, &what);
                let live = header == "$1\r\n";
                if live {
                    assert_eq!(reply_line(&mut client, &what), "v\r\n", "{what}");
                } else {
                    assert_eq!(header, "$-1\r\n", "{what}");
                }
                alive.push(live);
            }
            let later = time_reply(&mut client, &what);
            if later != now || now < start || now >= start + 100 {
                continue;
            }
            let expected = (0..100)
                .map(|offset| start + offset >= now)
                .collect::<Vec<_>>();
            assert_eq!(
                alive, expected,
                "{what}: keys expiring from {start} on, read at {now}"
            );
            placed = true;
            break;
        }
        assert!(
            placed,
            "{what}: no batch was read at one reading within its keys"
        );
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}");
    }
}

/// [PRE-2] firn's expiring context removes keys no command reads:
/// a thousand keys set with `PX 50` leave `DBSIZE`, which reads no key, at
/// zero within three seconds, as do a key `RENAME` moved and one `COPY` made,
/// whose expiries are queued under their new names.
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
        for request in [
            vec!["SET", "mover", "v", "PX", "200"],
            vec!["RENAME", "mover", "moved"],
            vec!["SET", "copier", "v", "PX", "200"],
            vec!["COPY", "copier", "copied"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("send the keys");
        let mut expected = b"+OK\r\n".repeat(KEYS);
        expected.extend_from_slice(b"+OK\r\n+OK\r\n+OK\r\n:1\r\n");
        expect_replies(&mut client, &expected, &what);
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
/// its expiry passed is absent rather than counting again from one. A key a
/// command finds expired is removed, and the file records the removal, as
/// Redis propagates it, so that the commands after it replay as they ran: a
/// `SET` with NX, one with KEEPTTL, an `INCR`, `APPEND`, `SETRANGE`, `MSETNX`
/// and `INCRBYFLOAT` on a key set already expired, a `SET` with NX after
/// `EXISTS`, `GET`, `TTL`, `TYPE`, `DEL`, `PERSIST`, `EXPIRE`, a negative
/// `EXPIRE`, `GETDEL`, `GETEX`, `STRLEN`, `GETRANGE`, `MGET`, or `RENAME` or
/// `COPY` from it, found it so, and a `RENAMENX` and a `COPY` onto it hold
/// their values after the restart, the `INCR` counting from zero and KEEPTTL,
/// as `INCRBYFLOAT`'s record has it, keeping no expiry. Each expiry given
/// relative to the time, by `SET` with EX or PX,
/// `SETEX`, `PSETEX`, `GETEX`, `EXPIRE` with an option and `PEXPIRE`, keeps
/// the very calendar millisecond `PEXPIRETIME` gave before the restart, which
/// a file recording the relative amount would replay later; a string
/// `APPEND` and `SETRANGE` edited holds its bytes; an expiry moves with its
/// key under `RENAME` and is copied with it by `COPY`; and a sum `INCRBYFLOAT`
/// wrote keeps its text and the key its expiry. The first run ends once its
/// one client has closed, after its writer appended and synced the last
/// changes.
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
            vec!["INCRBY", "count", "10"],
            vec!["DECRBY", "count", "3"],
            vec!["DECR", "count"],
            vec!["DEL", "gone"],
            vec!["SET", "kept", "v", "PX", "60000"],
            vec!["PERSIST", "kept"],
            vec!["SET", "later", "v", "PX", "60000"],
            vec!["SET", "persisted", "v", "PX", "300"],
            vec!["PERSIST", "persisted"],
            vec!["SET", "bumped", "5", "PX", "300"],
            vec!["INCR", "bumped"],
            vec!["SET", "lazy:a", "old", "PXAT", "1"],
            vec!["SET", "lazy:a", "new", "NX"],
            vec!["SET", "lazy:b", "5", "PXAT", "1"],
            vec!["INCR", "lazy:b"],
            vec!["SET", "lazy:c", "old", "PXAT", "1"],
            vec!["EXISTS", "lazy:c"],
            vec!["SET", "lazy:c", "new", "NX"],
            vec!["SET", "lazy:d", "old", "PXAT", "1"],
            vec!["GET", "lazy:d"],
            vec!["SET", "lazy:d", "new", "NX"],
            vec!["SET", "lazy:e", "old", "PXAT", "1"],
            vec!["SET", "lazy:e", "new", "KEEPTTL"],
            vec!["SET", "lazy:f", "old", "PXAT", "1"],
            vec!["TTL", "lazy:f"],
            vec!["SET", "lazy:f", "new", "NX"],
            vec!["SET", "lazy:g", "old", "PXAT", "1"],
            vec!["TYPE", "lazy:g"],
            vec!["SET", "lazy:g", "new", "NX"],
            vec!["SET", "lazy:h", "old", "PXAT", "1"],
            vec!["DEL", "lazy:h"],
            vec!["SET", "lazy:h", "new", "NX"],
            vec!["SET", "lazy:i", "old", "PXAT", "1"],
            vec!["PERSIST", "lazy:i"],
            vec!["SET", "lazy:i", "new", "NX"],
            vec!["SET", "lazy:j", "old", "PXAT", "1"],
            vec!["EXPIRE", "lazy:j", "100"],
            vec!["SET", "lazy:j", "new", "NX"],
            vec!["SET", "lazy:k", "old", "PXAT", "1"],
            vec!["GETDEL", "lazy:k"],
            vec!["SET", "lazy:k", "new", "NX"],
            vec!["SET", "lazy:l", "old", "PXAT", "1"],
            vec!["GETEX", "lazy:l", "PERSIST"],
            vec!["SET", "lazy:l", "new", "NX"],
            vec!["SET", "lazy:m", "old", "PXAT", "1"],
            vec!["EXPIRE", "lazy:m", "-1"],
            vec!["SET", "lazy:m", "new", "NX"],
            vec!["SET", "lazy:n", "old", "PXAT", "1"],
            vec!["APPEND", "lazy:n", "x"],
            vec!["SET", "lazy:o", "old", "PXAT", "1"],
            vec!["SETRANGE", "lazy:o", "0", "x"],
            vec!["SET", "lazy:p", "old", "PXAT", "1"],
            vec!["STRLEN", "lazy:p"],
            vec!["SET", "lazy:p", "new", "NX"],
            vec!["SET", "lazy:q", "old", "PXAT", "1"],
            vec!["GETRANGE", "lazy:q", "0", "-1"],
            vec!["SET", "lazy:q", "new", "NX"],
            vec!["SET", "lazy:r", "old", "PXAT", "1"],
            vec!["MGET", "lazy:r"],
            vec!["SET", "lazy:r", "new", "NX"],
            vec!["SET", "lazy:s", "old", "PXAT", "1"],
            vec!["MSETNX", "lazy:s", "new"],
            vec!["SET", "lazy:t", "5", "PXAT", "1"],
            vec!["INCRBYFLOAT", "lazy:t", "0.5"],
            vec!["SET", "lazy:u", "old", "PXAT", "1"],
            vec!["RENAME", "lazy:u", "lazy:u2"],
            vec!["SET", "lazy:u", "new", "NX"],
            vec!["SET", "lazy:v", "old", "PXAT", "1"],
            vec!["SET", "lazy:vs", "v"],
            vec!["RENAMENX", "lazy:vs", "lazy:v"],
            vec!["SET", "lazy:w", "old", "PXAT", "1"],
            vec!["COPY", "lazy:w", "lazy:w2"],
            vec!["SET", "lazy:w", "new", "NX"],
            vec!["SET", "lazy:x", "old", "PXAT", "1"],
            vec!["SET", "lazy:xs", "v"],
            vec!["COPY", "lazy:xs", "lazy:x"],
            vec!["SETEX", "at:setex", "100", "v"],
            vec!["PSETEX", "at:psetex", "100000", "v"],
            vec!["SET", "at:getex", "v"],
            vec!["GETEX", "at:getex", "EX", "100"],
            vec!["SET", "at:expire", "v"],
            vec!["EXPIRE", "at:expire", "100", "NX"],
            vec!["SET", "at:pexpire", "v"],
            vec!["PEXPIRE", "at:pexpire", "100000"],
            vec!["SET", "edited", "ab"],
            vec!["APPEND", "edited", "cd"],
            vec!["SETRANGE", "edited", "1", "XY"],
            vec!["MSETNX", "pair:a", "1", "pair:b", "2"],
            vec!["SET", "mv:src", "v", "PXAT", "99999999999999"],
            vec!["RENAME", "mv:src", "mv:dst"],
            vec!["COPY", "mv:dst", "mv:copy"],
            vec!["SET", "float", "10.5", "PXAT", "99999999999999"],
            vec!["INCRBYFLOAT", "float", "0.1"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("send the changes");
        expect_replies(
            &mut client,
            b"+OK\r\n+OK\r\n+OK\r\n:1\r\n:2\r\n:12\r\n:9\r\n:8\r\n:1\r\n+OK\r\n:1\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n:6\r\n+OK\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n$-1\r\n+OK\r\n+OK\r\n+OK\r\n+OK\r\n:-2\r\n+OK\r\n+OK\r\n+none\r\n+OK\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n$-1\r\n+OK\r\n+OK\r\n$-1\r\n+OK\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n:1\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n$0\r\n\r\n+OK\r\n+OK\r\n*1\r\n$-1\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n$3\r\n0.5\r\n+OK\r\n-ERR no such key\r\n+OK\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n:0\r\n+OK\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n+OK\r\n+OK\r\n$1\r\nv\r\n+OK\r\n:1\r\n+OK\r\n:1\r\n+OK\r\n:4\r\n:4\r\n:1\r\n+OK\r\n+OK\r\n:1\r\n+OK\r\n$4\r\n10.6\r\n",
            &what,
        );
        let timed = [
            "long",
            "later",
            "at:setex",
            "at:psetex",
            "at:getex",
            "at:expire",
            "at:pexpire",
        ];
        let mut expiries = Vec::new();
        for key in timed {
            client
                .write_all(&resp(&["PEXPIRETIME", key]))
                .expect("ask the expiry");
            expiries.push(integer_reply(&mut client, &what));
        }
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
            vec!["GET", "lazy:a"],
            vec!["GET", "lazy:b"],
            vec!["TTL", "lazy:b"],
            vec!["GET", "lazy:c"],
            vec!["GET", "lazy:d"],
            vec!["GET", "lazy:e"],
            vec!["TTL", "lazy:e"],
            vec!["GET", "lazy:f"],
            vec!["GET", "lazy:g"],
            vec!["GET", "lazy:h"],
            vec!["GET", "lazy:i"],
            vec!["GET", "lazy:j"],
            vec!["GET", "lazy:k"],
            vec!["GET", "lazy:l"],
            vec!["GET", "lazy:m"],
            vec!["GET", "lazy:n"],
            vec!["GET", "lazy:o"],
            vec!["GET", "lazy:p"],
            vec!["GET", "lazy:q"],
            vec!["GET", "lazy:r"],
            vec!["GET", "lazy:s"],
            vec!["GET", "lazy:t"],
            vec!["GET", "lazy:u"],
            vec!["GET", "lazy:v"],
            vec!["GET", "lazy:w"],
            vec!["GET", "lazy:x"],
            vec!["GET", "edited"],
            vec!["MGET", "pair:a", "pair:b"],
            vec!["PEXPIRETIME", "mv:dst"],
            vec!["PEXPIRETIME", "mv:copy"],
            vec!["EXISTS", "mv:src"],
            vec!["GET", "float"],
            vec!["PEXPIRETIME", "float"],
        ] {
            batch.extend(resp(&request));
        }
        client.write_all(&batch).expect("read the replayed keys");
        expect_replies(
            &mut client,
            b"$-1\r\n$-1\r\n$1\r\nv\r\n$1\r\n8\r\n$1\r\nv\r\n:-1\r\n$1\r\nv\r\n:-1\r\n$-1\r\n:41\r\n$3\r\nnew\r\n$1\r\n1\r\n:-1\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n:-1\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$1\r\nx\r\n$1\r\nx\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\nnew\r\n$3\r\n0.5\r\n$3\r\nnew\r\n$1\r\nv\r\n$3\r\nnew\r\n$1\r\nv\r\n$4\r\naXYd\r\n*2\r\n$1\r\n1\r\n$1\r\n2\r\n:99999999999999\r\n:99999999999999\r\n:0\r\n$4\r\n10.6\r\n:99999999999999\r\n",
            &what,
        );
        for (key, expiry) in timed.iter().zip(&expiries) {
            client
                .write_all(&resp(&["PEXPIRETIME", key]))
                .expect("ask the replayed expiry");
            let replayed = integer_reply(&mut client, &what);
            assert_eq!(replayed, *expiry, "{what}: the expiry of {key}");
        }
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
/// `COPY` duplicates a set, a hash, a sorted set and a list whole, each copy
/// changed afterwards without changing its original, and with REPLACE puts a
/// list in place of a set; `RENAME` moves a hash over a list.
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
        vec!["COPY", "s", "s2"],
        vec!["SADD", "s2", "x"],
        vec!["SCARD", "s"],
        vec!["SCARD", "s2"],
        vec!["COPY", "h", "h2"],
        vec!["HSET", "h2", "f", "9"],
        vec!["HGET", "h", "f"],
        vec!["HGET", "h2", "f"],
        vec!["COPY", "z", "z2"],
        vec!["ZADD", "z2", "5", "q"],
        vec!["ZCARD", "z"],
        vec!["ZPOPMIN", "z2", "2"],
        vec!["RPUSH", "lst", "a", "b", "c"],
        vec!["COPY", "lst", "lst2"],
        vec!["RPOP", "lst2"],
        vec!["LRANGE", "lst", "0", "-1"],
        vec!["COPY", "lst", "s", "REPLACE"],
        vec!["TYPE", "s"],
        vec!["RENAME", "h", "lst"],
        vec!["TYPE", "lst"],
        vec!["EXISTS", "h"],
    ] {
        batch.extend(resp(&request));
    }
    batch.extend_from_slice(b"SET inline yes\r\nGET inline\r\n");
    client.write_all(&batch).expect("send the batch");
    expect_replies(
        &mut client,
        b":3\r\n:4\r\n*4\r\n$1\r\nz\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nc\r\n*2\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nz\r\n*2\r\n$1\r\nc\r\n$1\r\nb\r\n:1\r\n$1\r\na\r\n:0\r\n:3\r\n:1\r\n:2\r\n:2\r\n:0\r\n$1\r\n3\r\n$-1\r\n:3\r\n:0\r\n$1\r\n0\r\n*4\r\n$1\r\nc\r\n$1\r\n0\r\n$1\r\na\r\n$1\r\n1\r\n:1\r\n+OK\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n:1\r\n+set\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n+OK\r\n$1\r\n5\r\n-ERR unknown command 'NOPE', with args beginning with: 'a' 'b' \r\n-ERR wrong number of arguments for 'llen' command\r\n:1\r\n:1\r\n:2\r\n:3\r\n:1\r\n:0\r\n$1\r\n3\r\n$1\r\n9\r\n:1\r\n:1\r\n:1\r\n*4\r\n$1\r\nb\r\n$1\r\n2\r\n$1\r\nq\r\n$1\r\n5\r\n:3\r\n:1\r\n$1\r\nc\r\n*3\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nc\r\n:1\r\n+list\r\n+OK\r\n+hash\r\n:0\r\n+OK\r\n$3\r\nyes\r\n",
        "the value-type batch",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// The exact decimal expansion of `m / 2^k`, which has `k` digits after the
/// point: `m * 5^k` with the point placed `k` digits from its end.
#[cfg(target_os = "linux")]
fn dyadic_decimal(m: u64, k: usize) -> String {
    let mut digits: Vec<u8> = m.to_string().bytes().rev().map(|b| b - b'0').collect();
    for _ in 0..k {
        let mut carry = 0;
        for digit in &mut digits {
            let value = *digit * 5 + carry;
            *digit = value % 10;
            carry = value / 10;
        }
        if carry > 0 {
            digits.push(carry);
        }
    }
    digits.resize(digits.len().max(k + 1), 0);
    let text: String = digits
        .iter()
        .rev()
        .map(|digit| char::from(b'0' + digit))
        .collect();
    let (whole, fraction) = text.split_at(text.len() - k);
    format!("{whole}.{fraction}")
}

/// firn reads and writes sorted-set scores as Redis does. Each score is added
/// with `ZADD`, read back with `ZSCORE` and popped with `ZPOPMIN`. Decimal text
/// rounds to the nearest double with ties to even: at the two halfway points
/// past 2^53, at the 752-digit halfway point below the smallest subnormal,
/// which rounds to zero and is refused as Redis refuses an underflow, and at
/// the 768-digit halfway points on either side of the smallest normal double,
/// which round up to it and down to it. A nonzero digit after a halfway point
/// breaks its tie, also when it lies past the 800th significant digit, both
/// after hundreds of digits and after a halfway point of at most 19 digits
/// followed by zeros, and a digit taken away keeps the value below it.
/// Hexadecimal text rounds as decimal text does, a nonzero digit past the bits
/// kept breaking a tie, a subnormal rounding to the smallest one, and a value
/// rounded past the largest double refused, also with an exponent past the
/// range of i64;
/// infinities and arguments of 400 and 1,000 digits are read as strtod reads
/// them; NaN, overflow, a leading or trailing space, an empty argument and a
/// zero byte are refused. Every score is written as %.17g writes it, ties at
/// the seventeenth digit to even, in exponential notation below 10^-4 and from
/// 10^17; negative zero is kept as 0, as Redis keeps it in a sorted set it
/// encodes as a listpack. Members at infinite, subnormal, zero and equal scores
/// pop in Redis's order. An option word before the first score, in either case
/// and compared up to a zero byte as strcasecmp compares it, is a syntax
/// error, while one in a later score's place is an invalid score. The expected
/// replies are redis-server 7.0.15's.
#[cfg(target_os = "linux")]
#[test]
fn firn_reads_and_writes_scores_as_redis_does() {
    let half_smallest = dyadic_decimal(1, 1075);
    let half_smallest_above = format!("{half_smallest}1");
    let half_smallest_far_above = format!("{half_smallest}{}1", "0".repeat(60));
    let below_normal = dyadic_decimal((1 << 53) - 1, 1075);
    let below_normal_below = format!(
        "{}4{}",
        &below_normal[..below_normal.len() - 1],
        "9".repeat(20)
    );
    let above_normal = dyadic_decimal((1 << 53) + 1, 1075);
    let above_normal_far_above = format!("{above_normal}{}1", "0".repeat(40));
    let short_tie_far_above = format!("9007199254740993.{}1", "0".repeat(784));
    let scaled_tie_far_above = format!("5.{}1e22", "0".repeat(799));
    let long_zeros = format!("0.{}1e401", "0".repeat(400));
    let long_thirds = format!("3.{}", "3".repeat(1000));
    let cases: [(&str, Option<&str>); 49] = [
        ("1.5", Some("1.5")),
        ("-2.5e-3", Some("-0.0025000000000000001")),
        ("0.1", Some("0.10000000000000001")),
        ("1e23", Some("9.9999999999999992e+22")),
        ("9007199254740993", Some("9007199254740992")),
        ("9007199254740995", Some("9007199254740996")),
        ("2.2250738585072011e-308", Some("2.2250738585072009e-308")),
        ("4.9e-324", Some("4.9406564584124654e-324")),
        ("2.4703282292062327e-324", None),
        ("2.4703282292062328e-324", Some("4.9406564584124654e-324")),
        (&half_smallest, None),
        (&half_smallest_above, Some("4.9406564584124654e-324")),
        (&half_smallest_far_above, Some("4.9406564584124654e-324")),
        (&below_normal, Some("2.2250738585072014e-308")),
        (&below_normal_below, Some("2.2250738585072009e-308")),
        (&above_normal, Some("2.2250738585072014e-308")),
        (&above_normal_far_above, Some("2.2250738585072019e-308")),
        (&short_tie_far_above, Some("9007199254740994")),
        (&scaled_tie_far_above, Some("5.0000000000000004e+22")),
        ("1.7976931348623158e308", Some("1.7976931348623157e+308")),
        ("1.7976931348623159e308", None),
        ("1e-400", None),
        (&long_zeros, Some("1")),
        (&long_thirds, Some("3.3333333333333335")),
        ("inf", Some("inf")),
        ("-Infinity", Some("-inf")),
        ("nan", None),
        ("infinit", None),
        ("0x1.8p1", Some("3")),
        ("0x1.00000000000008p0", Some("1")),
        ("0x1.000000000000080001p0", Some("1.0000000000000002")),
        ("0x1.fffffffffffff8p1023", None),
        ("0x1.fffffffffffff8p99999999999999999999", None),
        ("0x3p-1076", Some("4.9406564584124654e-324")),
        ("0x1p-1075", None),
        ("1125899906842623.75", Some("1125899906842623.8")),
        ("1125899906842623.25", Some("1125899906842623.2")),
        ("0.0001", Some("0.0001")),
        ("0.00001", Some("1.0000000000000001e-05")),
        ("99999999999999984", Some("99999999999999984")),
        ("1e17", Some("1e+17")),
        ("-0", Some("0")),
        (" 1", None),
        ("1 ", None),
        ("1e", None),
        ("", None),
        ("1\0", None),
        ("+.5", Some("0.5")),
        ("007", Some("7")),
    ];
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1"]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the client's waits");
    let mut batch = Vec::new();
    let mut expected = Vec::new();
    for (score, written) in cases {
        batch.extend(resp(&["ZADD", "z", score, "m"]));
        batch.extend(resp(&["ZSCORE", "z", "m"]));
        batch.extend(resp(&["ZPOPMIN", "z"]));
        match written {
            Some(written) => expected.extend(
                format!(
                    ":1\r\n${length}\r\n{written}\r\n*2\r\n$1\r\nm\r\n${length}\r\n{written}\r\n",
                    length = written.len()
                )
                .bytes(),
            ),
            None => expected.extend_from_slice(b"-ERR value is not a valid float\r\n$-1\r\n*0\r\n"),
        }
    }
    batch.extend(resp(&[
        "ZADD", "o", "inf", "top", "-inf", "bottom", "1.5", "b", "1.5", "a", "0", "zero", "-0",
        "negative", "4.9e-324", "tiny", "-1e308", "low", "2.5e-3", "small",
    ]));
    batch.extend(resp(&["ZPOPMIN", "o", "20"]));
    let popped = [
        "bottom",
        "-inf",
        "low",
        "-1e+308",
        "negative",
        "0",
        "zero",
        "0",
        "tiny",
        "4.9406564584124654e-324",
        "small",
        "0.0025000000000000001",
        "a",
        "1.5",
        "b",
        "1.5",
        "top",
        "inf",
    ];
    expected.extend(format!(":9\r\n*{}\r\n", popped.len()).bytes());
    for item in popped {
        expected.extend(format!("${}\r\n{item}\r\n", item.len()).bytes());
    }
    batch.extend(resp(&["ZADD", "q", "nx", "m"]));
    batch.extend(resp(&["ZADD", "q", "Ch", "m"]));
    batch.extend(resp(&["ZADD", "q", "gt\0x", "m"]));
    batch.extend(resp(&["ZADD", "q", "1", "a", "nx", "b"]));
    batch.extend(resp(&["ZCARD", "q"]));
    expected.extend_from_slice(
        b"-ERR syntax error\r\n-ERR syntax error\r\n-ERR syntax error\r\n-ERR value is not a valid float\r\n:0\r\n",
    );
    client.write_all(&batch).expect("send the batch");
    expect_replies(&mut client, &expected, "the score batch");
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn keeps strings of at most 24 bytes inside the keyspace and longer ones
/// in allocations of their own. Keys, members, fields and values of exactly 24
/// bytes and of 25 bytes sharing those 24, and a key differing from them only
/// in its last byte, stay distinct in every kind of value, and a number that
/// `INCR` rewrites to a different length reads back whole. A string `APPEND`
/// or `SETRANGE` grows from 23 or 24 bytes past 24 reads back whole, as does
/// one `SETRANGE` creates on either side of the limit, its gap filled with
/// zero bytes, and `GETRANGE` and `STRLEN` read either form; the expected
/// replies are redis-server 7.0.15's to the same requests.
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
    let short_run = "a".repeat(23);
    let a23 = short_run.as_str();
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
        vec!["SET", "p", a23],
        vec!["APPEND", "p", "b"],
        vec!["APPEND", "p", "c"],
        vec!["GET", "p"],
        vec!["GETRANGE", "p", "22", "24"],
        vec!["STRLEN", "p"],
        vec!["SET", "q", v24],
        vec!["SETRANGE", "q", "24", "w"],
        vec!["GET", "q"],
        vec!["SETRANGE", "q", "0", "W"],
        vec!["GET", "q"],
        vec!["STRLEN", "q"],
        vec!["SETRANGE", "q", "30", "Z"],
        vec!["GET", "q"],
        vec!["APPEND", "q", "d"],
        vec!["SETRANGE", "r", "30", "end"],
        vec!["GET", "r"],
        vec!["SETRANGE", "gap", "3", "x"],
        vec!["GET", "gap"],
        vec!["GETRANGE", "gap", "-1", "-1"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the batch");
    expect_replies(
        &mut client,
        b"+OK\r\n+OK\r\n+OK\r\n$1\r\n1\r\n$1\r\n2\r\n$1\r\n3\r\n:2\r\n:3\r\n$1\r\n2\r\n$1\r\n3\r\n+OK\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n:9223372036854775807\r\n-ERR increment or decrement would overflow\r\n$19\r\n9223372036854775807\r\n+OK\r\n:-9223372036854775806\r\n$20\r\n-9223372036854775806\r\n+OK\r\n-ERR value is not an integer or out of range\r\n:3\r\n:3\r\n:1\r\n:2\r\n:1\r\n:1\r\n:2\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n$-1\r\n:3\r\n$1\r\n2\r\n:0\r\n*4\r\n$25\r\naaaaaaaaaaaaaaaaaaaaaaaxy\r\n$1\r\n0\r\n$24\r\naaaaaaaaaaaaaaaaaaaaaaax\r\n$1\r\n1\r\n:2\r\n*2\r\n$25\r\naaaaaaaaaaaaaaaaaaaaaaaxy\r\n$24\r\naaaaaaaaaaaaaaaaaaaaaaax\r\n:3\r\n:2\r\n:1\r\n+OK\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvv\r\n$24\r\nvvvvvvvvvvvvvvvvvvvvvvvv\r\n+OK\r\n:24\r\n:25\r\n$25\r\naaaaaaaaaaaaaaaaaaaaaaabc\r\n$3\r\nabc\r\n:25\r\n+OK\r\n:25\r\n$25\r\nvvvvvvvvvvvvvvvvvvvvvvvvw\r\n:25\r\n$25\r\nWvvvvvvvvvvvvvvvvvvvvvvvw\r\n:25\r\n:31\r\n$31\r\nWvvvvvvvvvvvvvvvvvvvvvvvw\x00\x00\x00\x00\x00Z\r\n:32\r\n:33\r\n$33\r\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00end\r\n:4\r\n$4\r\n\x00\x00\x00x\r\n$1\r\nx\r\n",
        "the strings around the inline length",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn answers the string commands as Redis does. `SET` reads its options as
/// Redis 7.0.15 reads them: NX and XX store only in place of an absent key or
/// over a live one; GET answers the string the key held, or nil, and refuses a
/// key of another kind, storing nothing; KEEPTTL keeps the key's expiry where a
/// plain `SET` removes it; a later EX replaces an earlier one; a word is
/// compared in either case up to a zero byte; an expiry option needs an amount;
/// an option beside one it excludes, or one only `GETEX` takes, is a syntax
/// error; and an absolute expiry already past leaves the key expired at once.
/// `SETNX`, `SETEX`, `PSETEX`, `GETSET` and `GETDEL` answer as Redis does, and
/// `GETEX` reads its amount only once it has found a live string, and removes
/// the key for an absolute expiry already past, which `DBSIZE` then no longer
/// counts. `APPEND` of nothing creates an empty string; `GETRANGE` and
/// `SUBSTR` count negative indices from the end, clamp both to the string and
/// answer the empty string for an absent key or a range ending before it
/// starts; `SETRANGE` of nothing changes nothing, refuses a negative offset,
/// and refuses a string past 512 MiB, its offset and length summed without
/// overflow; each refuses another kind. `INCRBY`, `DECR` and `DECRBY` add
/// with Redis's overflow checks, `DECRBY` refusing the least i64 before it
/// reads the key, and keep the key's expiry. `INCRBYFLOAT` adds in x87 long
/// double arithmetic and writes the sum as Redis's `%.17Lf` with its trailing
/// zeros dropped: 10.5 plus 0.1 is 10.6, where doubles would give
/// 10.59999999999999964; hexadecimal text reads; text halfway between two
/// long doubles reads to the even one both ways, unless a digit far after the
/// halfway point breaks the tie; a tie at the seventeenth digit after the
/// point writes to even both ways; a subnormal sum, and negative zero, write
/// 0; a sum near the largest double writes all 309 digits, and one past the
/// largest long double is refused, as are an infinity, NaN, a leading space,
/// a zero byte and a value that is not a number, after a key of another kind.
/// The expected replies are redis-server 7.0.15's to the same requests.
#[cfg(target_os = "linux")]
#[test]
fn firn_answers_string_commands_as_redis_does() {
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
        vec!["SET", "k", "v", "nx\0zz"],
        vec!["SET", "k", "v2", "xx", "GET"],
        vec!["SET", "k", "v3", "EX", "10", "EX", "20"],
        vec!["TTL", "k"],
        vec!["SET", "k", "v4", "NX", "XX"],
        vec!["SET", "k", "v4", "KEEPTTL", "EX", "5"],
        vec!["SET", "k", "v4", "EX", "5", "KEEPTTL"],
        vec!["SET", "k", "v5", "KEEPTTL"],
        vec!["TTL", "k"],
        vec!["SET", "k", "v6"],
        vec!["TTL", "k"],
        vec!["SET", "k", "v7", "EX"],
        vec!["SET", "k", "v7", "EX", "NX"],
        vec!["SET", "k", "v7", "EX", "0"],
        vec!["SET", "k", "v8", "GET", "GET"],
        vec!["LPUSH", "l", "a"],
        vec!["SET", "l", "x", "GET"],
        vec!["SET", "l", "x", "NX", "GET"],
        vec!["TYPE", "l"],
        vec!["SET", "n", "x", "NX", "GET"],
        vec!["SET", "n", "y", "NX", "GET"],
        vec!["SET", "absent", "y", "XX", "GET"],
        vec!["SET", "absent", "y", "XX"],
        vec!["EXISTS", "absent"],
        vec!["SET", "k", "v", "keepttl\0x"],
        vec!["SET", "k", "v", "GET", "PERSIST"],
        vec!["SET", "k", "v", ""],
        vec!["SET", "k", "v", "PXAT", "1"],
        vec!["GET", "k"],
        vec!["SET", "l", "x"],
        vec!["GET", "l"],
        vec!["SETNX", "k", "x"],
        vec!["SETNX", "k", "y"],
        vec!["SETEX", "s", "100", "v"],
        vec!["TTL", "s"],
        vec!["SETEX", "s", "0", "v"],
        vec!["SETEX", "s", "abc", "v"],
        vec!["PSETEX", "s", "-5", "v"],
        vec!["PSETEX", "s", "100000", "v"],
        vec!["TTL", "s"],
        vec!["GETSET", "s", "w"],
        vec!["TTL", "s"],
        vec!["GETSET", "nope", "w"],
        vec!["LPUSH", "l2", "a"],
        vec!["GETSET", "l2", "w"],
        vec!["GETDEL", "l2"],
        vec!["GETDEL", "nope"],
        vec!["GETDEL", "nope"],
        vec!["SET", "g", "val"],
        vec!["GETEX", "g"],
        vec!["GETEX", "g", "EX", "100"],
        vec!["TTL", "g"],
        vec!["GETEX", "g", "PERSIST"],
        vec!["TTL", "g"],
        vec!["GETEX", "g", "NX"],
        vec!["GETEX", "g", "EX", "abc"],
        vec!["GETEX", "missing", "EX", "abc"],
        vec!["GETEX", "l2", "EX", "abc"],
        vec!["GETEX", "g", "EX", "0"],
        vec!["GETEX", "g", "EX", "10", "PERSIST"],
        vec!["GETEX", "g", "PXAT", "1"],
        vec!["DBSIZE"],
        vec!["GETEX", "g"],
        vec!["SET", "r", "hello"],
        vec!["APPEND", "l2", "x"],
        vec!["APPEND", "newa", ""],
        vec!["EXISTS", "newa"],
        vec!["STRLEN", "nope"],
        vec!["STRLEN", "l2"],
        vec!["GETRANGE", "r", "0", "4"],
        vec!["GETRANGE", "r", "-3", "-1"],
        vec!["GETRANGE", "r", "-1", "-5"],
        vec!["GETRANGE", "r", "3", "1"],
        vec!["GETRANGE", "r", "0", "-100"],
        vec!["GETRANGE", "r", "-100", "100"],
        vec!["GETRANGE", "r", "100", "200"],
        vec!["GETRANGE", "r", "x", "1"],
        vec!["GETRANGE", "r", "1", "x"],
        vec!["GETRANGE", "nope", "0", "1"],
        vec!["GETRANGE", "nope", "x", "1"],
        vec!["GETRANGE", "l2", "0", "1"],
        vec![
            "GETRANGE",
            "r",
            "-9223372036854775808",
            "9223372036854775807",
        ],
        vec!["GETRANGE", "r", "-10", "-20"],
        vec!["GETRANGE", "r", "-20", "-10"],
        vec!["SUBSTR", "r", "1", "3"],
        vec!["SETRANGE", "r", "-1", "x"],
        vec!["SETRANGE", "r", "x", "x"],
        vec!["SETRANGE", "nope", "5", ""],
        vec!["EXISTS", "nope"],
        vec!["SETRANGE", "r", "99999999999", ""],
        vec!["SETRANGE", "nope", "536870911", "ab"],
        vec!["SETRANGE", "nope", "9223372036854775807", "ab"],
        vec!["SETRANGE", "r", "536870911", "ab"],
        vec!["SETRANGE", "l2", "0", ""],
        vec!["GET", "r"],
        vec!["INCRBY", "i", "5"],
        vec!["INCRBY", "i", "x"],
        vec!["INCRBY", "i", "-10"],
        vec!["DECR", "i"],
        vec!["DECR", "fresh"],
        vec!["DECRBY", "i", "-9223372036854775808"],
        vec!["DECRBY", "i", "9223372036854775802"],
        vec!["DECRBY", "i", "1"],
        vec!["INCRBY", "i", "-1"],
        vec!["INCRBY", "i", "9223372036854775807"],
        vec!["INCRBY", "l2", "1"],
        vec!["DECRBY", "l2", "-9223372036854775808"],
        vec!["DECR", "l2"],
        vec!["SET", "t", "5", "PXAT", "99999999999999"],
        vec!["INCRBY", "t", "2"],
        vec!["DECRBY", "t", "3"],
        vec!["PEXPIRETIME", "t"],
        vec!["INCRBY", "i", " 1"],
        vec!["SET", "fl", "10.5"],
        vec!["INCRBYFLOAT", "fl", "0.1"],
        vec!["INCRBYFLOAT", "fl", "-10.6"],
        vec!["SET", "fl2", "3.0e3"],
        vec!["INCRBYFLOAT", "fl2", "1.0e3"],
        vec!["SET", "fl3", "0x1p3"],
        vec!["INCRBYFLOAT", "fl3", "0x1.8p1"],
        vec!["SET", "fl4", "0.000003814697265625"],
        vec!["INCRBYFLOAT", "fl4", "0"],
        vec!["SET", "fl5", "0.000011444091796875"],
        vec!["INCRBYFLOAT", "fl5", "0"],
        vec!["INCRBYFLOAT", "nofl", "1e-4940"],
        vec!["GET", "nofl"],
        vec!["SET", "neg", "-0.0"],
        vec!["INCRBYFLOAT", "neg", "0"],
        vec!["INCRBYFLOAT", "fl", "inf"],
        vec!["INCRBYFLOAT", "fl", "nan"],
        vec!["INCRBYFLOAT", "fl", " 1"],
        vec!["INCRBYFLOAT", "fl", "1.5\0abc"],
        vec!["SET", "bad", "abc"],
        vec!["INCRBYFLOAT", "bad", "1"],
        vec!["INCRBYFLOAT", "l2", "1"],
        vec!["INCRBYFLOAT", "l2", "abc"],
        vec!["SET", "huge", "1.18973149535723176502e4932"],
        vec!["INCRBYFLOAT", "huge", "1.18973149535723176502e4932"],
        vec!["SET", "fe", "5", "PXAT", "99999999999999"],
        vec!["INCRBYFLOAT", "fe", "1.5"],
        vec!["PEXPIRETIME", "fe"],
        vec!["SET", "big", "15.5"],
        vec!["INCRBYFLOAT", "big", "1.7976931348623157e308"],
        vec!["INCRBYFLOAT", "tie1", "18446744073709551617"],
        vec!["INCRBYFLOAT", "tie2", "18446744073709551619"],
        vec![
            "INCRBYFLOAT",
            "tie3",
            "18446744073709551617.0000000000000000000000001",
        ],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the string batch");
    expect_replies(
        &mut client,
        b"+OK\r\n$1\r\nv\r\n+OK\r\n:20\r\n-ERR syntax error\r\n-ERR syntax error\r\n-ERR syntax error\r\n+OK\r\n:20\r\n+OK\r\n:-1\r\n-ERR syntax error\r\n-ERR value is not an integer or out of range\r\n-ERR invalid expire time in 'set' command\r\n$2\r\nv6\r\n:1\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n+list\r\n$-1\r\n$1\r\nx\r\n$-1\r\n$-1\r\n:0\r\n+OK\r\n-ERR syntax error\r\n-ERR syntax error\r\n+OK\r\n$-1\r\n+OK\r\n$1\r\nx\r\n:1\r\n:0\r\n+OK\r\n:100\r\n-ERR invalid expire time in 'setex' command\r\n-ERR value is not an integer or out of range\r\n-ERR invalid expire time in 'psetex' command\r\n+OK\r\n:100\r\n$1\r\nv\r\n:-1\r\n$-1\r\n:1\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n$1\r\nw\r\n$-1\r\n+OK\r\n$3\r\nval\r\n$3\r\nval\r\n:100\r\n$3\r\nval\r\n:-1\r\n-ERR syntax error\r\n-ERR value is not an integer or out of range\r\n$-1\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n-ERR invalid expire time in 'getex' command\r\n-ERR syntax error\r\n$3\r\nval\r\n:5\r\n$-1\r\n+OK\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n:0\r\n:1\r\n:0\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n$5\r\nhello\r\n$3\r\nllo\r\n$0\r\n\r\n$0\r\n\r\n$1\r\nh\r\n$5\r\nhello\r\n$0\r\n\r\n-ERR value is not an integer or out of range\r\n-ERR value is not an integer or out of range\r\n$0\r\n\r\n-ERR value is not an integer or out of range\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n$5\r\nhello\r\n$0\r\n\r\n$1\r\nh\r\n$3\r\nell\r\n-ERR offset is out of range\r\n-ERR value is not an integer or out of range\r\n:0\r\n:0\r\n:5\r\n-ERR string exceeds maximum allowed size (proto-max-bulk-len)\r\n-ERR string exceeds maximum allowed size (proto-max-bulk-len)\r\n-ERR string exceeds maximum allowed size (proto-max-bulk-len)\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n$5\r\nhello\r\n:5\r\n-ERR value is not an integer or out of range\r\n:-5\r\n:-6\r\n:-1\r\n-ERR decrement would overflow\r\n:-9223372036854775808\r\n-ERR increment or decrement would overflow\r\n-ERR increment or decrement would overflow\r\n:-1\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n-ERR decrement would overflow\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n+OK\r\n:7\r\n:4\r\n:99999999999999\r\n-ERR value is not an integer or out of range\r\n+OK\r\n$4\r\n10.6\r\n$1\r\n0\r\n+OK\r\n$4\r\n4000\r\n+OK\r\n$2\r\n11\r\n+OK\r\n$19\r\n0.00000381469726562\r\n+OK\r\n$19\r\n0.00001144409179688\r\n$1\r\n0\r\n$1\r\n0\r\n+OK\r\n$1\r\n0\r\n-ERR increment would produce NaN or Infinity\r\n-ERR value is not a valid float\r\n-ERR value is not a valid float\r\n-ERR value is not a valid float\r\n+OK\r\n-ERR value is not a valid float\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n+OK\r\n-ERR increment would produce NaN or Infinity\r\n+OK\r\n$3\r\n6.5\r\n:99999999999999\r\n+OK\r\n$309\r\n179769313486231569995921046774104434048386446944329178485314420765176628523124396354701868608387085564428793419425520689308708363451136055357897278026477617073498977686288394835618163294045594434929537447290627480669417384648879122776218333598953852832103282504751611691208117720159985926376802466863339536384\r\n$20\r\n18446744073709551616\r\n$20\r\n18446744073709551620\r\n$20\r\n18446744073709551618\r\n",
        "the string batch",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// [SHARE-1, SHARE-2] firn answers `DEL`, `EXISTS` and `MSET` naming a key
/// more than once as Redis does, each holding its keys' entries through a key
/// set, which keeps one element per key: `DEL` removes and counts such a key
/// once, `EXISTS` counts a live key once for each time it is named and an
/// absent one not at all, and `MSET` keeps the last value named for a key.
/// `MGET` answers a key named twice twice, in the order named, though its key
/// set holds the keys in byte order, and nil for an absent key or one of
/// another kind; `MSETNX` keeps the last value named for a key and stores
/// nothing when any key is live, of any kind, looking its keys up in the
/// order named and stopping at the first live one, as Redis does: a key
/// expired before the command stays in place for `DBSIZE` to count when it is
/// named after that one, and is removed when named before it, though it sorts
/// after it. The expiring context, which may remove that key at any time,
/// could upset the first count only between the two `DBSIZE` commands around
/// `MSETNX`, outside its statement, a window of microseconds, and cannot upset
/// the second. `RENAME` and `RENAMENX` naming
/// one key twice leave a live key as it is, answering OK and 0, and refuse an
/// absent one; `COPY` refuses one key named twice before it reads the key,
/// and `RENAMENX` and `COPY` without REPLACE leave a live destination alone.
/// `UNLINK` removes and counts a key named twice once, `TOUCH` counts it
/// twice. `COPY` reads its options in order, each up to a zero byte: DB takes
/// an integer of the int range, of which only 0 names firn's one database.
/// The expected replies are redis-server 7.0.15's to the same requests, a
/// server configured with one database for `COPY`'s DB.
#[cfg(target_os = "linux")]
#[test]
fn firn_answers_commands_naming_a_key_twice_as_redis_does() {
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
        vec!["SET", "a", "1"],
        vec!["SET", "b", "2"],
        vec!["SET", "c", "3"],
        vec!["EXISTS", "c", "a", "c", "absent", "c"],
        vec!["DEL", "a", "a", "absent", "b"],
        vec!["EXISTS", "a", "b", "a"],
        vec!["MSET", "k", "1", "j", "2", "k", "3", "j", "4", "k", "5"],
        vec!["GET", "k"],
        vec!["GET", "j"],
        vec!["DBSIZE"],
        vec!["MGET", "k", "c", "k", "absent", "j", "c"],
        vec!["LPUSH", "l", "v"],
        vec!["MGET", "l", "k"],
        vec!["MSETNX", "x", "1", "y", "2", "x", "3"],
        vec!["MGET", "x", "y"],
        vec!["MSETNX", "x", "9", "z", "9"],
        vec!["EXISTS", "z"],
        vec!["MSETNX", "l", "1"],
        vec!["RENAME", "c", "c"],
        vec!["RENAMENX", "c", "c"],
        vec!["RENAME", "absent", "absent"],
        vec!["COPY", "c", "c"],
        vec!["RENAME", "c", "k"],
        vec!["MGET", "c", "k"],
        vec!["RENAMENX", "k", "j"],
        vec!["COPY", "k", "j"],
        vec!["COPY", "k", "j", "REPLACE"],
        vec!["MGET", "k", "j"],
        vec!["UNLINK", "j", "j", "absent", "x"],
        vec!["TOUCH", "k", "k", "absent", "y"],
        vec!["COPY", "k", "m", "DB", "1"],
        vec!["COPY", "k", "m", "DB", "-1"],
        vec!["COPY", "k", "m", "DB", "x"],
        vec!["COPY", "k", "m", "DB", "4294967296"],
        vec!["COPY", "k", "m", "DB"],
        vec!["COPY", "k", "m", "FOO"],
        vec!["COPY", "k", "m", "DB", "0", "DB", "1"],
        vec!["COPY", "k", "m", "replace\0x", "db\0", "0"],
        vec!["MGET", "m"],
        vec!["DBSIZE"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the batch");
    expect_replies(
        &mut client,
        b"+OK\r\n+OK\r\n+OK\r\n:4\r\n:2\r\n:0\r\n+OK\r\n$1\r\n5\r\n$1\r\n4\r\n:3\r\n*6\r\n$1\r\n5\r\n$1\r\n3\r\n$1\r\n5\r\n$-1\r\n$1\r\n4\r\n$1\r\n3\r\n:1\r\n*2\r\n$-1\r\n$1\r\n5\r\n:1\r\n*2\r\n$1\r\n3\r\n$1\r\n2\r\n:0\r\n:0\r\n:0\r\n+OK\r\n:0\r\n-ERR no such key\r\n-ERR source and destination objects are the same\r\n+OK\r\n*2\r\n$-1\r\n$1\r\n3\r\n:0\r\n:0\r\n:1\r\n*2\r\n$1\r\n3\r\n$1\r\n3\r\n:2\r\n:3\r\n-ERR DB index is out of range\r\n-ERR DB index is out of range\r\n-ERR value is not an integer or out of range\r\n-ERR value is out of range, value must between -2147483648 and 2147483647\r\n-ERR syntax error\r\n-ERR syntax error\r\n-ERR DB index is out of range\r\n:1\r\n*1\r\n$1\r\n3\r\n:4\r\n",
        "the commands naming a key twice",
    );
    let mut late = resp(&["SET", "late", "1", "PXAT", "1"]);
    late.extend(resp(&["DBSIZE"]));
    late.extend(resp(&["MSETNX", "k", "1", "late", "2"]));
    late.extend(resp(&["DBSIZE"]));
    late.extend(resp(&["MSETNX", "late", "1", "k", "2"]));
    late.extend(resp(&["DBSIZE"]));
    client
        .write_all(&late)
        .expect("send MSETNX past a live key");
    expect_replies(&mut client, b"+OK\r\n", "an expired key");
    let before = integer_reply(&mut client, "DBSIZE before MSETNX");
    assert_eq!(integer_reply(&mut client, "MSETNX past a live key"), 0);
    let after = integer_reply(&mut client, "DBSIZE after MSETNX");
    assert_eq!(
        after, before,
        "MSETNX looked up a key named after a live one"
    );
    assert_eq!(integer_reply(&mut client, "MSETNX before a live key"), 0);
    assert_eq!(
        integer_reply(&mut client, "DBSIZE after the second MSETNX"),
        4,
        "MSETNX did not look up a key named before a live one"
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn answers `CONFIG` with too few arguments, an unpaired option or
/// several parameters, and echoes client bytes in its errors, as Redis does:
/// a zero byte ends an echoed name or argument, carriage return and line feed
/// become spaces so that an error stays one line, an unknown subcommand is
/// echoed to 128 bytes and an unknown `CONFIG SET` option whole, and a
/// carriage return a malformed request holds where a dollar sign belongs is
/// echoed as a space before the connection closes. The expected replies are
/// redis-server 7.0.15's to the same bytes.
#[cfg(target_os = "linux")]
#[test]
fn firn_answers_config_and_echoes_client_bytes_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"1"]);
    let mut client = connect_when_ready(port);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound the client's waits");
    let mut batch = b"*1\r\n$6\r\nCONFIG\r\n*2\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n*3\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$1\r\nx\r\n*5\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$1\r\nx\r\n$1\r\ny\r\n$1\r\nz\r\n*4\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$1\r\nx\r\n$1\r\ny\r\n*4\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$4\r\nsave\r\n$10\r\nappendonly\r\n*5\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$10\r\nappendonly\r\n$4\r\nsave\r\n$4\r\nsave\r\n*4\r\n$6\r\nCONFIG\r\n$3\r\nget\r\n$7\r\nnothing\r\n$4\r\nelse\r\n*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$4\r\nsave\r\n*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$10\r\nAPPENDONLY\r\n*3\r\n$6\r\nCONFIG\r\n$3\r\nGET\r\n$4\r\nSave\r\n*2\r\n$4\r\nGET\x00\r\n$1\r\na\r\n*3\r\n$4\r\nG\r\nT\r\n$3\r\nb\nc\r\n$3\r\nd\x00e\r\n*2\r\n$6\r\nCONFIG\r\n$4\r\nNO\nP\r\n".to_vec();
    // An unknown subcommand is echoed to 128 bytes; an unknown CONFIG SET
    // option is echoed whole, up to a zero byte.
    batch.extend_from_slice(b"*2\r\n$6\r\nCONFIG\r\n$200\r\n");
    batch.extend_from_slice(&[b'b'; 200]);
    batch.extend_from_slice(b"\r\n*4\r\n$6\r\nCONFIG\r\n$3\r\nSET\r\n$161\r\n");
    batch.extend_from_slice(&[b'c'; 150]);
    batch.push(0);
    batch.extend_from_slice(&[b'd'; 10]);
    batch.extend_from_slice(b"\r\n$1\r\nv\r\n*1\r\n\r\n");
    client.write_all(&batch).expect("send the batch");
    let mut expected = b"-ERR wrong number of arguments for 'config' command\r\n-ERR wrong number of arguments for 'config|get' command\r\n-ERR wrong number of arguments for 'config|set' command\r\n-ERR syntax error\r\n-ERR Unknown option or number of arguments for CONFIG SET - 'x'\r\n*4\r\n$10\r\nappendonly\r\n$2\r\nno\r\n$4\r\nsave\r\n$0\r\n\r\n*4\r\n$10\r\nappendonly\r\n$2\r\nno\r\n$4\r\nsave\r\n$0\r\n\r\n*0\r\n*2\r\n$4\r\nsave\r\n$0\r\n\r\n*2\r\n$10\r\nAPPENDONLY\r\n$2\r\nno\r\n*2\r\n$4\r\nSave\r\n$0\r\n\r\n-ERR unknown command 'GET', with args beginning with: 'a' \r\n-ERR unknown command 'G  T', with args beginning with: 'b c' 'd' \r\n-ERR unknown subcommand 'NO P'. Try CONFIG HELP.\r\n".to_vec();
    expected.extend_from_slice(b"-ERR unknown subcommand '");
    expected.extend_from_slice(&[b'b'; 128]);
    expected.extend_from_slice(
        b"'. Try CONFIG HELP.\r\n-ERR Unknown option or number of arguments for CONFIG SET - '",
    );
    expected.extend_from_slice(&[b'c'; 150]);
    expected.extend_from_slice(b"'\r\n-ERR Protocol error: expected '$', got ' '\r\n");
    expect_replies(&mut client, &expected, "the CONFIG and echo batch");
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
/// list keeps the elements its pushes and pops left, a hash its field, a
/// sorted set the member ZPOPMIN left at its score, and a member added at 0.1
/// keeps the double nearest 0.1, which the replay reads again from the score's
/// text as the command read it. The 25 of 50 members SPOP
/// removed stay removed, since the file records the pop as the SREM of the
/// members it chose, as Redis records it; a replay that popped at random, from
/// a generator seeded by the clock at each start, would almost surely remove
/// others. A key the expiring context removed is recorded as removed, as Redis
/// propagates it, so a `SET` with NX that found it absent holds its value after
/// the restart, where a replay keeping the expired key would refuse it.
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
        vec!["ZADD", "f", "0.1", "m"],
        add,
        vec!["SET", "lapse", "old", "PX", "1"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the changes");
    expect_replies(
        &mut client,
        b":3\r\n$1\r\na\r\n:1\r\n:2\r\n*2\r\n$1\r\na\r\n$1\r\n1\r\n:1\r\n:50\r\n+OK\r\n",
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
    // The expiring context, which wakes every 100 milliseconds, removes the
    // key set with PX 1 before this SET finds it absent.
    std::thread::sleep(Duration::from_millis(250));
    client
        .write_all(&resp(&["SET", "lapse", "new", "NX"]))
        .expect("set the expired key again");
    expect_replies(&mut client, b"+OK\r\n", "the key set again");
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
        vec!["ZSCORE", "f", "m"],
        vec!["SCARD", "s"],
        remove,
        vec!["GET", "lapse"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("read the replayed values");
    expect_replies(
        &mut client,
        b"*2\r\n$1\r\nb\r\n$1\r\nc\r\n$1\r\nv\r\n:1\r\n$1\r\n2\r\n$19\r\n0.10000000000000001\r\n:25\r\n:0\r\n$3\r\nnew\r\n",
        "the replayed values",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0, "the second run");
}

/// Reads until the server closes the connection, failing on anything it sends
/// first.
#[cfg(target_os = "linux")]
fn expect_closed(stream: &mut TcpStream, what: &str) {
    let mut rest = Vec::new();
    stream
        .read_to_end(&mut rest)
        .unwrap_or_else(|error| panic!("{what}: the connection stayed open: {error}"));
    assert!(
        rest.is_empty(),
        "{what}: {:?}",
        String::from_utf8_lossy(&rest)
    );
}

/// Fails when the server sends anything within a third of a second.
#[cfg(target_os = "linux")]
fn expect_silence(stream: &mut TcpStream, what: &str) {
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .expect("bound the wait for silence");
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte) {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) => {}
        other => panic!("{what}: expected no reply, got {other:?} {byte:?}"),
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("restore the reply wait");
}

/// firn splits an inline command as Redis's sdssplitargs splits one: double
/// quotes in which \n, \r, \t, \b, \a, \\, \" and \xHH stand for their bytes,
/// a backslash before any other byte for that byte and an incomplete \x for x;
/// single quotes in which only \' stands for a quote; a quote opening in the
/// middle of an argument; a vertical tab that separates arguments only before
/// an argument starts; an empty quoted argument; and a quoted command name,
/// the decoded arguments echoed by an unknown command's error. A closing quote
/// followed by anything but white space is an unbalanced quote, answered with
/// Redis's error before the connection closes, the PING after it unanswered,
/// and so is a double or single quote the line leaves open. The expected bytes
/// are redis-server 7.0.15's for the same bytes.
#[cfg(target_os = "linux")]
#[test]
fn firn_splits_quoted_inline_arguments_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"3"]);
    let mut client = connect_when_ready(port);
    client
        .write_all(b"ECHO \"hello world\"\r\nECHO \"a\\nb\\r\\t\\b\\a\\\\\\\"\\x41\\x4a\\xzz\\q\"\r\nECHO 'it\\'s'\r\nECHO 'a\\nb'\r\nECHO a\"b c\"\r\nECHO a\x0bb\r\nECHO \x0ba\r\nECHO \"a\"\x0bb\r\nECHO \"\"\r\n\"ECHO\" x\r\nECHO \"\\x4\"\r\nNOPE \"a b\" 'c\\'d' e\r\nECHO \"abc\"x\r\nPING\r\n")
        .expect("send the inline batch");
    expect_replies(
        &mut client,
        b"$11\r\nhello world\r\n$15\r\na\nb\r\t\x08\x07\\\"AJxzzq\r\n$4\r\nit's\r\n$4\r\na\\nb\r\n$4\r\nab c\r\n$3\r\na\x0bb\r\n$1\r\na\r\n-ERR wrong number of arguments for 'echo' command\r\n$0\r\n\r\n$1\r\nx\r\n$2\r\nx4\r\n-ERR unknown command 'NOPE', with args beginning with: 'a b' 'c'd' 'e' \r\n-ERR Protocol error: unbalanced quotes in request\r\n",
        "the inline batch",
    );
    expect_closed(&mut client, "after the unbalanced quote");
    drop(client);
    for line in [
        &b"ECHO \"abc\r\nPING\r\n"[..],
        &b"ECHO 'abc\r\nPING\r\n"[..],
    ] {
        let what = format!("{:?}", String::from_utf8_lossy(line));
        let mut client = connect_when_ready(port);
        client.write_all(line).expect("send a quote left open");
        expect_replies(
            &mut client,
            b"-ERR Protocol error: unbalanced quotes in request\r\n",
            &what,
        );
        expect_closed(&mut client, &what);
    }
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn reads a request's count and length lines as Redis 7.0 reads them. A
/// carriage return ends a line whatever byte follows it, which is skipped
/// unread; a negative count is skipped; and an array of 2,147,483,647
/// elements is a request still arriving, the most Redis takes, while
/// 2,147,483,648 elements, a count that is not a number and a negative length
/// are malformed and close the connection. A zero byte before a line's
/// carriage return leaves the line incomplete, because Redis finds the
/// carriage return with strchr: with 65,536 bytes held from the line's first
/// byte the connection waits, and one more byte is answered as a count, or a
/// length, line too big before the connection closes. The expected bytes are
/// redis-server 7.0.15's.
#[cfg(target_os = "linux")]
#[test]
fn firn_reads_count_and_length_lines_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"6"]);
    let mut client = connect_when_ready(port);
    client
        .write_all(b"*1\r\n$4\rXPING\r\n*1\rX$4\r\nPING\r\n*-1\r\n*2147483647\r\n")
        .expect("send lines of every shape");
    expect_replies(&mut client, b"+PONG\r\n+PONG\r\n", "the lines' shapes");
    expect_silence(&mut client, "an array of 2,147,483,647 elements");
    drop(client);
    for (line, error) in [
        (
            &b"*2147483648\r\n"[..],
            &b"-ERR Protocol error: invalid multibulk length\r\n"[..],
        ),
        (
            &b"*-abc\r\n"[..],
            &b"-ERR Protocol error: invalid multibulk length\r\n"[..],
        ),
        (
            &b"*1\r\n$-1\r\n"[..],
            &b"-ERR Protocol error: invalid bulk length\r\n"[..],
        ),
    ] {
        let what = format!("{:?}", String::from_utf8_lossy(line));
        let mut client = connect_when_ready(port);
        client.write_all(line).expect("send a malformed line");
        expect_replies(&mut client, error, &what);
        expect_closed(&mut client, &what);
    }
    for (head, error) in [
        (
            &b"*1\x00\r\n"[..],
            &b"-ERR Protocol error: too big mbulk count string\r\n"[..],
        ),
        (
            &b"*1\r\n$4\x00\r\n"[..],
            &b"-ERR Protocol error: too big bulk count string\r\n"[..],
        ),
    ] {
        let what = format!("a zero byte in {:?}", String::from_utf8_lossy(head));
        let line_start = if head.starts_with(b"*1\r\n") { 4 } else { 0 };
        let mut client = connect_when_ready(port);
        let mut held = head.to_vec();
        held.resize(line_start + 65_536, b'x');
        client
            .write_all(&held)
            .expect("send 65,536 bytes from the line");
        expect_silence(&mut client, &what);
        client.write_all(b"x").expect("send one byte more");
        expect_replies(&mut client, error, &what);
        expect_closed(&mut client, &what);
    }
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn answers CONFIG GET for the parameters it reports as Redis 7.0 does,
/// matching an argument that holds [, * or ? as Redis's stringmatchlen
/// matches a pattern, in either case: * reaches all eight; an argument
/// naming a parameter and a pattern reaching it answer it once, spelled as the
/// first argument spells it; a class with a range, ? and a negated class
/// match; the range [Z-a] holds nothing, because Redis swaps a reversed
/// range's bounds before folding their case; a backslash outside a class
/// folds case and inside one does not; a pattern stops at a zero byte; and an
/// argument with none of the three bytes is a name, \Port among them. The
/// options --timeout and --appendfilename set the values reported. Every reply
/// holds what redis-server 7.0.15 answers for these parameters with the same
/// settings, in alphabetical order, one of the orders Redis answers in.
#[cfg(target_os = "linux")]
#[test]
fn firn_matches_config_get_patterns_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(
        true,
        &[
            b"--port",
            text.as_bytes(),
            b"--clients",
            b"1",
            b"--timeout",
            b"7",
            b"--appendfilename",
            b"patterns.aof",
        ],
    );
    let mut client = connect_when_ready(port);
    let mut batch = Vec::new();
    for request in [
        vec!["CONFIG", "GET", "*"],
        vec!["CONFIG", "GET", "a*"],
        vec!["CONFIG", "GET", "APPENDONLY", "a*"],
        vec!["CONFIG", "GET", "[b-d]*"],
        vec!["CONFIG", "GET", "p?rt"],
        vec!["CONFIG", "GET", "*o*t"],
        vec!["CONFIG", "GET", "[^a-r]*"],
        vec!["CONFIG", "GET", "[Z-a]*"],
        vec!["CONFIG", "GET", "\\Port"],
        vec!["CONFIG", "GET", "\\Port*"],
        vec!["CONFIG", "GET", "[\\P]ort"],
        vec!["CONFIG", "GET", "s*\0x"],
        vec!["CONFIG", "GET", "maxmemory"],
        vec!["CONFIG", "GET", "save", "SAVE", "s*"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the patterns");
    let port_field = format!("$4\r\nport\r\n${}\r\n{text}\r\n", text.len());
    let expected = format!(
        "*16\r\n$14\r\nappendfilename\r\n$12\r\npatterns.aof\r\n$10\r\nappendonly\r\n$2\r\nno\r\n$4\r\nbind\r\n$9\r\n127.0.0.1\r\n$9\r\ndatabases\r\n$1\r\n1\r\n{port_field}$11\r\nrequirepass\r\n$0\r\n\r\n$4\r\nsave\r\n$0\r\n\r\n$7\r\ntimeout\r\n$1\r\n7\r\n\
         *4\r\n$14\r\nappendfilename\r\n$12\r\npatterns.aof\r\n$10\r\nappendonly\r\n$2\r\nno\r\n\
         *4\r\n$14\r\nappendfilename\r\n$12\r\npatterns.aof\r\n$10\r\nAPPENDONLY\r\n$2\r\nno\r\n\
         *4\r\n$4\r\nbind\r\n$9\r\n127.0.0.1\r\n$9\r\ndatabases\r\n$1\r\n1\r\n\
         *2\r\n{port_field}\
         *4\r\n{port_field}$7\r\ntimeout\r\n$1\r\n7\r\n\
         *4\r\n$4\r\nsave\r\n$0\r\n\r\n$7\r\ntimeout\r\n$1\r\n7\r\n\
         *0\r\n\
         *0\r\n\
         *2\r\n{port_field}\
         *0\r\n\
         *2\r\n$4\r\nsave\r\n$0\r\n\r\n\
         *0\r\n\
         *2\r\n$4\r\nsave\r\n$0\r\n\r\n"
    );
    expect_replies(&mut client, expected.as_bytes(), "the patterns");
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn takes options by name, in either case, a later value replacing an
/// earlier one: with --bind 127.0.0.2 it listens on that address, so a
/// connection to 127.0.0.1 on the same port is refused, and CONFIG GET
/// reports the address, the port the last --port named, the append-only file
/// --appendonly yes turned on under the name --appendfilename gave, and the
/// idle limit --timeout set. An unknown option, an option without its value, a
/// port past 65,535, an address that is not one, an --appendonly other than
/// yes or no, an idle limit past 2,147,483,647 seconds, an empty file name, a
/// fifth argument by position and an argument by position after one by name
/// each stop firn with status 1 before it listens.
#[cfg(target_os = "linux")]
#[test]
fn firn_listens_where_its_options_by_name_say() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(
        true,
        &[
            b"--port",
            b"1",
            b"--bind",
            b"127.0.0.2",
            b"--PORT",
            text.as_bytes(),
            b"--Clients",
            b"1",
            b"--appendonly",
            b"yes",
            b"--appendfilename",
            b"options.aof",
            b"--timeout",
            b"9",
        ],
    );
    let mut client = connect_to_when_ready(SocketAddr::from(([127, 0, 0, 2], port)));
    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
    assert!(
        TcpStream::connect_timeout(&loopback, Duration::from_millis(500)).is_err(),
        "firn listens on 127.0.0.1 too"
    );
    let mut batch = resp(&["PING"]);
    batch.extend(resp(&[
        "CONFIG",
        "GET",
        "bind",
        "port",
        "appendonly",
        "appendfilename",
        "timeout",
    ]));
    client.write_all(&batch).expect("ask where firn listens");
    expect_replies(
        &mut client,
        format!(
            "+PONG\r\n*10\r\n$14\r\nappendfilename\r\n$11\r\noptions.aof\r\n$10\r\nappendonly\r\n$3\r\nyes\r\n$4\r\nbind\r\n$9\r\n127.0.0.2\r\n$4\r\nport\r\n${}\r\n{text}\r\n$7\r\ntimeout\r\n$1\r\n9\r\n",
            text.len()
        )
        .as_bytes(),
        "the options' values",
    );
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
    let refused: [&[&[u8]]; 9] = [
        &[b"--nosuch", b"1"],
        &[b"--port"],
        &[b"--port", b"65536"],
        &[b"--bind", b"127.0.0"],
        &[b"--appendonly", b"maybe"],
        &[b"--timeout", b"2147483648"],
        &[b"--appendfilename", b""],
        &[b"6379", b"0", b"-", b"0", b"5"],
        &[b"--port", b"6379", b"0"],
    ];
    for arguments in refused {
        let child = program.spawn_on_route(true, arguments);
        let (status, _) = finished(child);
        assert_eq!(status, 1, "{arguments:?}");
    }
}

/// A restarted firn listens on its port while the connection its first run
/// closed waits out TIME_WAIT there, as Redis does, which takes SO_REUSEADDR
/// on the runtime's listening socket: the first run answers QUIT with OK and
/// closes the connection itself, leaving the server's side of it in TIME_WAIT
/// on the port, and a second run started at once on the same port accepts a
/// client and answers it. Without the option the second run's listen fails
/// and firn stops with status 3. The option still refuses a listen of an
/// address and port another socket listens on: a third run started while the
/// second listens stops with status 3.
#[cfg(target_os = "linux")]
#[test]
fn firn_listens_again_on_its_port_after_a_restart() {
    let program = firn();
    for native_ring in [true, false] {
        let what = format!("native ring: {native_ring}");
        let port = free_port();
        let text = port.to_string();
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1"]);
        let mut client = connect_when_ready(port);
        client.write_all(&resp(&["QUIT"])).expect("send QUIT");
        expect_replies(&mut client, b"+OK\r\n", &what);
        expect_closed(&mut client, &what);
        drop(client);
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}: the first run");
        let child = program.spawn_on_route(native_ring, &[text.as_bytes(), b"2"]);
        let mut client = connect_when_ready(port);
        let third = program.spawn_on_route(native_ring, &[text.as_bytes(), b"1"]);
        let (status, _) = finished(third);
        assert_eq!(
            status, 3,
            "{what}: a third run on a port the second listens on"
        );
        client.write_all(&resp(&["PING"])).expect("send a ping");
        expect_replies(&mut client, b"+PONG\r\n", &what);
        drop(client);
        drop(connect_when_ready(port));
        let (status, _) = finished(child);
        assert_eq!(status, 0, "{what}: the second run");
    }
}

/// firn requires the password --requirepass names as Redis does. A connection
/// that has not authenticated is answered NOAUTH for every command but AUTH,
/// HELLO and QUIT, after an unknown command or a wrong count of arguments is
/// answered as such; a wrong password, one of the right length among them,
/// the user default with a wrong one and another user are answered WRONGPASS
/// and leave it locked; the password
/// unlocks it, and a wrong one afterwards leaves it unlocked. HELLO with AUTH
/// unlocks a connection too and answers with its id. While locked, an array of
/// more than 10 elements or a bulk string of more than 16,384 bytes is a
/// protocol error that closes the connection, and once unlocked an array of
/// 11 elements is a command. The expected bytes are redis-server 7.0.15's with
/// the same password, the connection's id aside.
#[cfg(target_os = "linux")]
#[test]
fn firn_requires_its_password_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"4", b"--requirepass", b"secret"]);
    let mut client = connect_when_ready(port);
    let mut batch = Vec::new();
    let mut eleven = vec!["DEL"];
    let keys = (0..10).map(|index| format!("k{index}")).collect::<Vec<_>>();
    eleven.extend(keys.iter().map(String::as_str));
    for request in [
        vec!["PING"],
        vec!["NOPE", "a"],
        vec!["GET"],
        vec!["CONFIG", "GET"],
        vec!["CONFIG", "GET", "save"],
        vec!["CLIENT", "FOO"],
        vec!["HELLO", "2"],
        vec!["AUTH", "wrong"],
        vec!["AUTH", "secreT"],
        vec!["AUTH", "default", "wrong"],
        vec!["AUTH", "bob", "secret"],
        vec!["PING"],
        vec!["AUTH", "secret"],
        vec!["PING"],
        vec!["AUTH", "wrong"],
        vec!["PING"],
        vec!["CONFIG", "GET", "requirepass"],
        eleven,
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the locked batch");
    expect_replies(
        &mut client,
        b"-NOAUTH Authentication required.\r\n-ERR unknown command 'NOPE', with args beginning with: 'a' \r\n-ERR wrong number of arguments for 'get' command\r\n-ERR wrong number of arguments for 'config|get' command\r\n-NOAUTH Authentication required.\r\n-ERR unknown subcommand 'FOO'. Try CLIENT HELP.\r\n-NOAUTH HELLO must be called with the client already authenticated, otherwise the HELLO AUTH <user> <pass> option can be used to authenticate the client and select the RESP protocol version at the same time\r\n-WRONGPASS invalid username-password pair or user is disabled.\r\n-WRONGPASS invalid username-password pair or user is disabled.\r\n-WRONGPASS invalid username-password pair or user is disabled.\r\n-WRONGPASS invalid username-password pair or user is disabled.\r\n-NOAUTH Authentication required.\r\n+OK\r\n+PONG\r\n-WRONGPASS invalid username-password pair or user is disabled.\r\n+PONG\r\n*2\r\n$11\r\nrequirepass\r\n$6\r\nsecret\r\n:0\r\n",
        "the locked batch",
    );
    drop(client);
    let mut client = connect_when_ready(port);
    let mut batch = resp(&["HELLO", "2", "AUTH", "default", "secret"]);
    batch.extend(resp(&["PING"]));
    client.write_all(&batch).expect("authenticate with HELLO");
    expect_replies(
        &mut client,
        b"*14\r\n$6\r\nserver\r\n$5\r\nredis\r\n$7\r\nversion\r\n$6\r\n7.0.15\r\n$5\r\nproto\r\n:2\r\n$2\r\nid\r\n:2\r\n$4\r\nmode\r\n$10\r\nstandalone\r\n$4\r\nrole\r\n$6\r\nmaster\r\n$7\r\nmodules\r\n*0\r\n+PONG\r\n",
        "HELLO with AUTH",
    );
    drop(client);
    let mut client = connect_when_ready(port);
    client
        .write_all(&resp(&[
            "ECHO", "a", "a", "a", "a", "a", "a", "a", "a", "a", "a",
        ]))
        .expect("send 11 elements while locked");
    expect_replies(
        &mut client,
        b"-ERR Protocol error: unauthenticated multibulk length\r\n",
        "11 elements while locked",
    );
    expect_closed(&mut client, "11 elements while locked");
    drop(client);
    let mut client = connect_when_ready(port);
    client
        .write_all(b"*2\r\n$4\r\nECHO\r\n$16385\r\n")
        .expect("send a long bulk string while locked");
    expect_replies(
        &mut client,
        b"-ERR Protocol error: unauthenticated bulk length\r\n",
        "16,385 bytes while locked",
    );
    expect_closed(&mut client, "16,385 bytes while locked");
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}

/// firn answers the connection commands as Redis does. CLIENT ID counts the
/// connections from 1 in the order firn accepted them, and HELLO reports the
/// same id; CLIENT SETNAME gives a name, refuses one with a space keeping the
/// old one, and removes it with the empty name; CLIENT SETINFO, which Redis
/// 7.0 does not have, is an unknown subcommand; HELLO answers RESP2's map with
/// no version or version 2 and refuses 1 and 3 as unsupported, since firn
/// speaks RESP2 alone, and its SETNAME names the connection, an option's name
/// read up to a zero byte as Redis's strcasecmp reads it; AUTH without a
/// configured password is answered with Redis's error for the password alone
/// and succeeds for the user default; SELECT takes 0 alone, as Redis does with
/// one database; COMMAND and COMMAND COUNT describe no command and COMMAND
/// DOCS is an unknown subcommand; TIME answers the calendar time; and QUIT
/// answers OK and closes the connection, leaving the request after it
/// unanswered. The expected bytes are redis-server 7.0.15's, started with one
/// database, but for the ids and HELLO 3, which Redis would answer in RESP3.
#[cfg(target_os = "linux")]
#[test]
fn firn_answers_connection_commands_as_redis_does() {
    let program = firn();
    let port = free_port();
    let text = port.to_string();
    let child = program.spawn_on_route(true, &[text.as_bytes(), b"2"]);
    let mut client = connect_when_ready(port);
    let mut batch = Vec::new();
    for request in [
        vec!["CLIENT", "ID"],
        vec!["CLIENT", "GETNAME"],
        vec!["CLIENT", "SETNAME", "conn-1"],
        vec!["CLIENT", "GETNAME"],
        vec!["CLIENT", "SETNAME", "a b"],
        vec!["CLIENT", "GETNAME"],
        vec!["CLIENT", "SETNAME", ""],
        vec!["CLIENT", "GETNAME"],
        vec!["CLIENT", "SETINFO", "lib-name", "x"],
        vec!["CLIENT", "ID", "x"],
        vec!["CLIENT"],
        vec!["HELLO"],
        vec!["HELLO", "3"],
        vec!["HELLO", "1"],
        vec!["HELLO", "x"],
        vec!["HELLO", "2", "SETNAME", "via-hello"],
        vec!["CLIENT", "GETNAME"],
        vec!["HELLO", "2", "FOO"],
        vec!["HELLO", "2", "SETNAME\0x", "via-zero"],
        vec!["CLIENT", "GETNAME"],
        vec!["HELLO", "2", "AUTH\0x", "default", "x"],
        vec!["AUTH", "x"],
        vec!["AUTH", "default", "x"],
        vec!["SELECT", "0"],
        vec!["SELECT", "1"],
        vec!["SELECT", "abc"],
        vec!["SELECT", "3000000000"],
        vec!["COMMAND"],
        vec!["COMMAND", "COUNT"],
        vec!["COMMAND", "COUNT", "x"],
        vec!["COMMAND", "DOCS"],
        vec!["TIME", "x"],
    ] {
        batch.extend(resp(&request));
    }
    client.write_all(&batch).expect("send the connection batch");
    let hello = "*14\r\n$6\r\nserver\r\n$5\r\nredis\r\n$7\r\nversion\r\n$6\r\n7.0.15\r\n$5\r\nproto\r\n:2\r\n$2\r\nid\r\n:1\r\n$4\r\nmode\r\n$10\r\nstandalone\r\n$4\r\nrole\r\n$6\r\nmaster\r\n$7\r\nmodules\r\n*0\r\n";
    let expected = format!(
        ":1\r\n$-1\r\n+OK\r\n$6\r\nconn-1\r\n-ERR Client names cannot contain spaces, newlines or special characters.\r\n$6\r\nconn-1\r\n+OK\r\n$-1\r\n-ERR unknown subcommand 'SETINFO'. Try CLIENT HELP.\r\n-ERR wrong number of arguments for 'client|id' command\r\n-ERR wrong number of arguments for 'client' command\r\n{hello}-NOPROTO unsupported protocol version\r\n-NOPROTO unsupported protocol version\r\n-ERR Protocol version is not an integer or out of range\r\n{hello}$9\r\nvia-hello\r\n-ERR Syntax error in HELLO option 'FOO'\r\n{hello}$8\r\nvia-zero\r\n{hello}-ERR AUTH <password> called without any password configured for the default user. Are you sure your configuration is correct?\r\n+OK\r\n+OK\r\n-ERR DB index is out of range\r\n-ERR value is not an integer or out of range\r\n-ERR value is out of range, value must between -2147483648 and 2147483647\r\n*0\r\n:0\r\n-ERR wrong number of arguments for 'command|count' command\r\n-ERR unknown subcommand 'DOCS'. Try COMMAND HELP.\r\n-ERR wrong number of arguments for 'time' command\r\n"
    );
    expect_replies(&mut client, expected.as_bytes(), "the connection batch");
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the host clock is past 1970")
        .as_secs();
    client.write_all(&resp(&["TIME"])).expect("ask the time");
    assert_eq!(reply_line(&mut client, "TIME"), "*2\r\n");
    assert_eq!(reply_line(&mut client, "TIME's seconds"), "$10\r\n");
    let seconds = reply_line(&mut client, "TIME's seconds");
    let seconds = seconds.trim_end().parse::<u64>().expect("TIME's seconds");
    assert!(
        seconds + 5 >= before && seconds <= before + 5,
        "{seconds} against {before}"
    );
    let length = reply_line(&mut client, "TIME's microseconds");
    let micro = reply_line(&mut client, "TIME's microseconds");
    let micro = micro.trim_end();
    assert_eq!(length, format!("${}\r\n", micro.len()));
    assert!(micro.parse::<u64>().expect("TIME's microseconds") < 1_000_000);
    let mut batch = resp(&["QUIT"]);
    batch.extend(resp(&["PING"]));
    client.write_all(&batch).expect("send QUIT and a ping");
    expect_replies(&mut client, b"+OK\r\n", "QUIT");
    expect_closed(&mut client, "after QUIT");
    drop(client);
    let mut client = connect_when_ready(port);
    client
        .write_all(&resp(&["CLIENT", "ID"]))
        .expect("ask the second connection's id");
    expect_replies(&mut client, b":2\r\n", "the second connection's id");
    drop(client);
    let (status, _) = finished(child);
    assert_eq!(status, 0);
}
