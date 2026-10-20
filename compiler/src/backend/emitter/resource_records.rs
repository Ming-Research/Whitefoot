//! Resource-exhaustion record writers emitted into the module: the sequential and latched variants for POSIX and Windows.

use super::*;

/// Retry an interrupted POSIX diagnostic write without changing the caller's
/// cursor. The supported Darwin and Linux ABIs both number EINTR as four but
/// expose the thread-local errno cell through different accessors. This stays
/// in the emitted module, including when no floor runtime is linked.
pub(super) fn emit_posix_resource_write(
    output: &mut Module,
    target: TargetLayout,
) -> Result<(), BackendFailure> {
    let errno = if target.triple().contains("apple-darwin") {
        "__error"
    } else {
        "__errno_location"
    };
    output.declare(Signature::new(
        "write",
        "i64",
        vec![
            Parameter::unnamed("i32"),
            Parameter::unnamed("ptr"),
            Parameter::unnamed("i64"),
        ],
    ));
    output.declare(Signature::new(errno, "ptr", Vec::new()));
    output.text("\n");
    let mut signature = Signature::new(
        "wf_resource_write",
        "i64",
        vec![
            Parameter::named("ptr", "%bytes"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write\n", &[]);
    body.open_block("write".to_owned());
    body.instructions("  %written = call i64 @write(i32 2, ptr %bytes, i64 %length)\n  %failed = icmp slt i64 %written, 0\n  br i1 %failed, label %error, label %done\n", &["write"]);
    body.open_block("error".to_owned());
    body.instructions(&format!("  %errno = call ptr @{errno}()\n  %code = load i32, ptr %errno, align 4\n  %interrupted = icmp eq i32 %code, 4\n  br i1 %interrupted, label %write, label %done\n"), &[errno]);
    body.open_block("done".to_owned());
    body.instructions("  ret i64 %written\n", &[]);
    output.define(signature.define(body, "")?);
    output.text("\n");
    Ok(())
}

/// The heap-resource record writer of a module with one thread.
///
/// The thread that reaches it writes its complete record to standard error and
/// aborts the process without unwinding. There is no one to arbitrate with, so
/// there is no latch: these are the bytes every module emitted before the
/// overlapped world existed, and they are what a default build still gets.
pub(super) fn sequential_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write.loop\n", &[]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf_resource_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// Windows twin of [`sequential_resource_record_writer`]. The private runtime
/// call writes the same bytes to the process diagnostic channel without
/// importing the POSIX file-descriptor ABI into a COFF module.
pub(super) fn windows_sequential_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write.loop\n", &[]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf__windows_diagnostic_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf__windows_diagnostic_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// The module's own answer for the shared record latch, and the only state the
/// resource-record path carries.
///
/// The latch a record writer takes is the floor runtime's, because the floor's
/// signal handler writes the stack record and this module writes every other
/// one: a latch each would leave the two classes unserialized against each
/// other, and two threads dying of different resources at once could interleave
/// two records on one channel. Asking the floor for the address is what makes
/// "no execution writes a second one" a mechanism rather than an argument.
///
/// The `weak` definition here is the same standalone answer
/// [`floor::floor_runtime_fallback`] gives: an emitted module must link and run
/// without the floor's translation unit, and the real definition replaces this
/// one whenever that unit is linked, which is every ordinary build. Zero until
/// some thread writes a record, and no path outside the writer reads it, so a
/// program that writes none pays nothing for it.
pub(super) fn resource_record_latch() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.global(
        ".wf_resource_record.latch".to_owned(),
        "global",
        "i32".to_owned(),
        "0".to_owned(),
        Some(4),
        References::default(),
    );
    Ok(module)
}

/// The module's standalone definition of the shared latch's accessor.
pub(super) fn resource_record_latch_fallback() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new("wf__floor_record_latch", "ptr", vec![]);
    signature.linkage = Linkage::Weak;
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions(
        "  ret ptr @.wf_resource_record.latch\n",
        &[".wf_resource_record.latch"],
    );
    module.define(signature.define(body, "")?);
    Ok(module)
}

/// [`sequential_resource_record_writer`]'s work under a first-writer-wins latch,
/// emitted
/// where the module can have more than one thread inside it — that is, where
/// it writes a heap-resource record and hands a call out.
///
/// The first thread to arrive takes the shared latch and owns the record: it
/// writes its complete bytes to standard error and aborts the process without
/// unwinding. Every other thread that arrives while the latch is taken parks,
/// and the winner's abort takes it down with the process. The latch is the
/// floor runtime's, not this module's, so the parking also holds between this
/// writer and the floor's own — an execution that runs out of stack on one
/// thread while another is refused an allocation still produces exactly one
/// well-formed record rather than two interleaved ones. [PAR-1]'s
/// erroneous-execution guarantee is met by construction.
///
/// *Which* record wins may depend on the schedule, and that is the whole of
/// what a permitted overlap can change about one. The bytes are fixed by the
/// resource class that wins — no worker, thread, or dynamic stack appears in
/// them — so a run that exhausts nothing observes nothing here.
///
/// The park spins on a *volatile* load rather than an empty loop, so no
/// optimizer may delete the loop and let a losing thread fall through into a
/// second record.
pub(super) fn latched_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  %latch = call ptr @wf__floor_record_latch()\n  %acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst\n  %won = extractvalue { i32, i1 } %acquired, 1\n  br i1 %won, label %write.loop, label %park\n", &["wf__floor_record_latch"]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf_resource_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    body.open_block("park".to_owned());
    body.instructions(
        "  %parked = load volatile i32, ptr %latch, align 4\n  br label %park\n",
        &[],
    );
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// Windows twin of [`latched_resource_record_writer`], sharing the floor
/// runtime's first-writer latch while using the native diagnostic channel.
pub(super) fn windows_latched_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  %latch = call ptr @wf__floor_record_latch()\n  %acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst\n  %won = extractvalue { i32, i1 } %acquired, 1\n  br i1 %won, label %write.loop, label %park\n", &["wf__floor_record_latch"]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf__windows_diagnostic_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf__windows_diagnostic_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    body.open_block("park".to_owned());
    body.instructions(
        "  %parked = load volatile i32, ptr %latch, align 4\n  br label %park\n",
        &[],
    );
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}
