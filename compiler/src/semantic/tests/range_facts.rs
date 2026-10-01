//! Range facts [RANGE-1..RANGE-5]: formation, discharge and the counted
//! loop certificate.

use crate::SemanticOutcome;

use super::with_semantics;

#[test]
#[ignore = "prints checked bodies for inspection"]
fn dump_checked_bodies() {
    let path = std::env::var("WF_DUMP").expect("WF_DUMP names a source file");
    let source = std::fs::read(path).expect("source reads");
    with_semantics(&source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("dump fixture must check: {outcome:?}");
        };
        for function in program.data.executable_functions() {
            println!("== {}\n{:#?}\n{:#?}", function.name, function.range_facts, function.body);
        }
    });
}
