//! RustPython backend smoke (Python alternative, CPython-free). Installs the
//! generated native module into a RustPython interpreter and runs Python that
//! exercises the read surface. Run: `cargo test -p autoagent-bingen
//! --features py-rustpython --test rustpython`.
#![cfg(feature = "py-rustpython")]

use rustpython_vm::{compiler::Mode, Interpreter};

#[test]
fn rustpython_module_runs_read_surface() {
    let builder = Interpreter::builder(Default::default());
    let def = autoagent_bingen::python::python_bingen::module_def(&builder.ctx);
    let interp = builder.add_native_module(def).build();

    interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        // No stdlib is initialized, so check the JSON string directly.
        let src = r#"
import autoagent
assert autoagent.version() == 1, "version should be 1"
report = autoagent.doctor("/tmp")
assert "checks" in report, "doctor must return checks"
assert report.startswith("{"), "doctor must return a JSON object string"
"#;
        let code = vm
            .compile(src, Mode::Exec, "<rustpython-smoke>".to_owned())
            .expect("compile python smoke");
        vm.run_code_obj(code, scope).expect("run python smoke");
    });
}
