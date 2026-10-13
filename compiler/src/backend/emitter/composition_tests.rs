use super::*;

fn emitted_model(model: Module) -> LlvmModule {
    LlvmModule {
        text: model.render(),
        model,
        threaded_layout: false,
        ledger: Vec::new(),
    }
}

#[test]
fn inconsistent_attributes_stop_whole_module_composition_and_cache_rendering() {
    let mut model = Module::default();
    model.attribute_group(1, "noreturn".into());
    let mut llvm = emitted_model(model);
    let mut child = Module::default();
    child.attribute_group(1, "nounwind".into());
    assert_eq!(llvm.append(child), Err(BackendFailure::InvalidIr));
    assert!(LlvmModule::decode(&llvm.encode()).is_none());
}

#[test]
fn inconsistent_declarations_stop_whole_module_composition_and_cache_rendering() {
    let mut model = Module::default();
    model.declare(Signature::new("callee", "void", Vec::new()));
    let mut llvm = emitted_model(model);
    let mut child = Module::default();
    child.declare(Signature::new("callee", "i32", Vec::new()));
    assert_eq!(llvm.append(child), Err(BackendFailure::InvalidIr));
    assert!(LlvmModule::decode(&llvm.encode()).is_none());
}
