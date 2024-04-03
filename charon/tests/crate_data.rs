#![feature(rustc_private)]

use std::{error::Error, fs::File};

use charon_lib::{export::GCrateData, llbc_ast};

fn translate(
    code: impl std::fmt::Display,
) -> Result<GCrateData<llbc_ast::FunDecl, llbc_ast::GlobalDecl>, Box<dyn Error>> {
    use charon_lib::driver::CharonCallbacks;
    use charon_lib::{export, logger};

    // Initialize the logger
    logger::initialize_logger();

    // Write the code to a temporary file.
    use std::io::Write;
    let tmp_dir = tempfile::TempDir::new()?;
    let file_path = tmp_dir.path().join("test_crate.rs");
    {
        let mut tmp_file = File::create(&file_path)?;
        write!(tmp_file, "{}", code)?;
        drop(tmp_file);
    }

    // Call the Rust compiler with our custom callback.
    let mut callback = CharonCallbacks::new(Default::default());
    let args = vec![file_path.to_string_lossy().into_owned()];
    let res = callback.run_compiler(args);
    // Extract the computed crate data.
    assert_eq!(callback.error_count, 0);
    assert!(res.is_ok());
    let export::CrateData::LLBC(crate_data) = callback.crate_data.unwrap() else {
        panic!("expected llbc data, got ullbc instead")
    };

    Ok(crate_data)
}

#[test]
fn type_decl() -> Result<(), Box<dyn Error>> {
    let crate_data = translate(
        "
        struct Struct;
        fn main() {}
        ",
    )?;
    assert_eq!(
        serde_json::to_string(&crate_data.types[0].name.name).unwrap(),
        r#"[{"Ident":["test_crate",0]},{"Ident":["Struct",0]}]"#,
    );
    Ok(())
}

#[test]
fn attributes() -> Result<(), Box<dyn Error>> {
    // Use the `clippy::` prefix because it's ignored by rustc.
    let crate_data = translate(
        r#"
        #[clippy::foo]
        #[clippy::foo(arg)]
        #[clippy::foo = "arg"]
        struct Struct;

        #[non_exhaustive]
        enum Enum {}

        #[clippy::foo]
        trait Trait {}

        #[clippy::foo]
        impl Trait for Struct {}

        #[clippy::foo]
        const FOO: () = ();

        #[clippy::foo]
        static BAR: () = ();

        #[inline(never)]
        fn main() {}
        "#,
    )?;
    assert_eq!(
        crate_data.types[0].attributes,
        vec!["clippy::foo", "clippy::foo(arg)", "clippy::foo = \"arg\""]
    );
    assert_eq!(crate_data.types[1].attributes, vec!["non_exhaustive"]);
    assert_eq!(crate_data.trait_decls[0].attributes, vec!["clippy::foo"]);
    assert_eq!(crate_data.trait_impls[0].attributes, vec!["clippy::foo"]);
    assert_eq!(crate_data.globals[0].attributes, vec!["clippy::foo"]);
    assert_eq!(crate_data.globals[1].attributes, vec!["clippy::foo"]);
    assert_eq!(crate_data.functions[0].attributes, vec!["inline(never)"]);
    Ok(())
}
