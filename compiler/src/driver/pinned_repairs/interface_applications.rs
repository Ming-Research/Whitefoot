//! The repair for an interface application written with a literal in an argument list [FN-3].

use super::RepairPair;

pub(super) const INTERFACE_APPLICATIONS: &[RepairPair] = &[
    // [FN-3] an interface application in an argument list forwards declared
    // binders; a literal there is refused at the application. One
    // alternative names a binding, the other forwards a declared binder.
    RepairPair {
        name: "fn3-neg-literal-in-interface-application.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn3-neg-literal-in-interface-application.wf"
        ),
        rule: "FN-3",
        sentences: &[
            "]: UnforwardedGroupApplication\n",
            "\n  mechanical_fix: to forward a group in scope, write in order the binders its `interface` introduction declares; to supply concrete arguments, declare a `binding` for this interface application and write the binding's name in place of the application\n",
        ],
        repaired: &[
            br#"interface Sized<const n: u8> {
}

binding Hundred : Sized<100> {
}

fn inner<interface Sized<j>>() -> r: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let v = inner::<Hundred>();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"interface Sized<const n: u8> {
}

binding Hundred : Sized<100> {
}

fn inner<interface Sized<j>>() -> r: u64 pure {
  return 0_u64;
}

fn outer<interface Sized<k>>() -> r: u64 pure {
  return inner::<Sized<k>>();
}

fn main() -> status: std::process::ExitStatus pure {
  let v = outer::<Hundred>();
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
