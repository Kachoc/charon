#![feature(rustc_private)]

use assert_cmd::prelude::{CommandCargoExt, OutputAssertExt};
use itertools::Itertools;
use std::{error::Error, fs::File, io::BufReader, process::Command};

use charon_lib::{
    export::CrateData,
    logger,
    meta::InlineAttr,
    names::{Name, PathElem},
    types::TypeDecl,
    values::ScalarValue,
};

fn translate(code: impl std::fmt::Display) -> Result<CrateData, Box<dyn Error>> {
    // Initialize the logger
    logger::initialize_logger();

    // Write the code to a temporary file.
    use std::io::Write;
    let tmp_dir = tempfile::TempDir::new()?;
    let input_path = tmp_dir.path().join("test_crate.rs");
    {
        let mut tmp_file = File::create(&input_path)?;
        write!(tmp_file, "{}", code)?;
        drop(tmp_file);
    }

    // Call charon
    let output_path = tmp_dir.path().join("test_crate.llbc");
    Command::cargo_bin("charon")?
        .arg("--no-cargo")
        .arg("--input")
        .arg(input_path)
        .arg("--dest-file")
        .arg(&output_path)
        .assert()
        .try_success()?;

    // Extract the computed crate data.
    let crate_data: CrateData = {
        let file = File::open(output_path)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)?
    };

    Ok(crate_data)
}

/// `Name` is a complex datastructure; to inspect it we serialize it a little bit.
fn repr_name(n: &Name) -> String {
    n.name
        .iter()
        .map(|path_elem| match path_elem {
            PathElem::Ident(i, _) => i,
            PathElem::Impl(_) => "<impl>",
        })
        .join("::")
}

#[test]
fn type_decl() -> Result<(), Box<dyn Error>> {
    let crate_data = translate(
        "
        struct Struct;
        fn main() {}
        ",
    )?;
    assert_eq!(repr_name(&crate_data.types[0].name), "test_crate::Struct");
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
        crate_data.types[0].item_meta.attributes,
        vec!["clippy::foo", "clippy::foo(arg)", "clippy::foo = \"arg\""]
    );
    assert_eq!(
        crate_data.types[1].item_meta.attributes,
        vec!["non_exhaustive"]
    );
    assert_eq!(
        crate_data.trait_decls[0].item_meta.attributes,
        vec!["clippy::foo"]
    );
    assert_eq!(
        crate_data.trait_impls[0].item_meta.attributes,
        vec!["clippy::foo"]
    );
    assert_eq!(
        crate_data.globals[0].item_meta.attributes,
        vec!["clippy::foo"]
    );
    assert_eq!(
        crate_data.globals[1].item_meta.attributes,
        vec!["clippy::foo"]
    );
    assert_eq!(
        crate_data.functions[0].item_meta.attributes,
        vec!["inline(never)"]
    );
    assert_eq!(
        crate_data.functions[0].item_meta.inline,
        Some(InlineAttr::Never)
    );
    Ok(())
}

#[test]
fn visibility() -> Result<(), Box<dyn Error>> {
    let crate_data = translate(
        r#"
        pub struct Pub;
        struct Priv;

        mod private {
            pub struct PubInPriv;
        }
        "#,
    )?;
    assert_eq!(repr_name(&crate_data.types[0].name), "test_crate::Pub");
    assert!(crate_data.types[0].item_meta.public);
    assert_eq!(repr_name(&crate_data.types[1].name), "test_crate::Priv");
    assert!(!crate_data.types[1].item_meta.public);
    // Note how we think `PubInPriv` is public. It kind of is but there is no path to it. This is
    // probably fine.
    assert_eq!(
        repr_name(&crate_data.types[2].name),
        "test_crate::private::PubInPriv"
    );
    assert!(crate_data.types[2].item_meta.public);
    Ok(())
}

#[test]
fn discriminants() -> Result<(), Box<dyn Error>> {
    let crate_data = translate(
        r#"
        enum Foo {
            Variant1,
            Variant2,
        }
        #[repr(u32)]
        enum Bar {
            Variant1 = 3,
            Variant2 = 42,
        }
        "#,
    )?;
    fn get_enum_discriminants(ty: &TypeDecl) -> Vec<ScalarValue> {
        ty.kind.as_enum().iter().map(|v| v.discriminant).collect()
    }
    assert_eq!(
        get_enum_discriminants(&crate_data.types[0]),
        vec![ScalarValue::Isize(0), ScalarValue::Isize(1)]
    );
    assert_eq!(
        get_enum_discriminants(&crate_data.types[1]),
        vec![ScalarValue::U32(3), ScalarValue::U32(42)]
    );
    Ok(())
}

#[test]
fn rename_attribute() -> Result<(), Box<dyn Error>> {
    let crate_data = translate(
        r#"
        #![feature(register_tool)]
        #![register_tool(charon)]
        #![register_tool(aeneas)]
        
        #[charon::rename("BoolTest")]
        pub trait BoolTrait {
            // Required method
            #[charon::rename("getTest")]
            fn get_bool(&self) -> bool;

            // Provided method
            #[charon::rename("retTest")]
            fn ret_true(&self) -> bool {
                true
            }
        }

        #[charon::rename("BoolImpl")]
        impl BoolTrait for bool {
            fn get_bool(&self) -> bool {
                *self
            }
        }

        #[charon::rename("BoolFn")]
        fn test_bool_trait<T>(x: bool) -> bool {
            x.get_bool() && x.ret_true()
        }

        #[charon::rename("TypeTest")]
        type Test = i32;

        #[charon::rename("VariantsTest")]
        enum SimpleEnum {
            #[charon::rename("Variant1")]
            FirstVariant,
            SecondVariant,
            ThirdVariant,
        }

        #[charon::rename("StructTest")]
        struct Foo {
            #[charon::rename("FieldTest")]
            field1: u32,
        }

        #[charon::rename("Const_Test")]
        const C: u32 = 100 + 10 + 1;

        #[aeneas::rename("Type-Aeneas36")]
        type Test2 = u32;
        "#,
    )?;

    assert_eq!(
        crate_data.trait_decls[0].item_meta.rename,
        Some("BoolTest".to_string())
    );

    assert_eq!(
        crate_data.functions[2].item_meta.rename,
        Some("getTest".to_string())
    );

    assert_eq!(
        crate_data.functions[3].item_meta.rename,
        Some("retTest".to_string())
    );

    assert_eq!(
        crate_data.trait_impls[0].item_meta.rename,
        Some("BoolImpl".to_string())
    );

    assert_eq!(
        crate_data.functions[1].item_meta.rename,
        Some("BoolFn".to_string())
    );

    assert_eq!(
        crate_data.types[0].item_meta.rename,
        Some("TypeTest".to_string())
    );

    assert_eq!(
        crate_data.types[1].item_meta.rename,
        Some("VariantsTest".to_string())
    );

    let enumvec = crate_data.types[1].kind.as_enum();
    let variant = enumvec.get(0.into());
    assert_eq!(
        variant.unwrap().item_meta.rename,
        Some("Variant1".to_string())
    );

    assert_eq!(
        crate_data.types[2].item_meta.rename,
        Some("StructTest".to_string())
    );

    assert_eq!(
        crate_data.globals[0].item_meta.rename,
        Some("Const_Test".to_string())
    );

    assert_eq!(
        crate_data.types[3].item_meta.rename,
        Some("Type-Aeneas36".to_string())
    );

    let fieldvec = crate_data.types[2].kind.as_struct();
    let field = fieldvec.get(0.into());
    assert_eq!(
        field.unwrap().item_meta.rename,
        Some("FieldTest".to_string())
    );
    Ok(())
}
