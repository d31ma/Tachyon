#![allow(clippy::expect_used)]

use super::registry::NativeCompanionInput;
use crate::project::NativeCompanion;

struct KotlinHelperProject;

impl KotlinHelperProject {
    const RATE: &'static str = "@Helper\nclass TaxRateHelper {\n    fun rate(): Int = 2\n}\n";
    const TAX: &'static str = "@Helper\nclass TaxHelper {\n    @Needs\n    val rates: TaxRateHelper\n\n    fun addTax(subtotal: Int): Int = subtotal + rates.rate()\n}\n";
    const SERVICE: &'static str = "@Service\nclass CheckoutService {\n    @Needs\n    val helper: TaxHelper\n\n    fun total(): Int = helper.addTax(40)\n}\n";

    fn registry(tax: &str, browser: bool) -> Result<String, crate::Failure> {
        let root = tempfile::tempdir().expect("project");
        for (relative, text) in [
            ("client/helpers/TaxRateHelper.kt", Self::RATE),
            ("client/helpers/TaxHelper.kt", tax),
            ("client/services/CheckoutService.kt", Self::SERVICE),
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
            std::fs::write(path, text).expect("file");
        }
        super::registry::source(
            &[NativeCompanionInput {
                snapshot_root: root.path().to_owned(),
                language: NativeCompanion::Kotlin,
                source: root.path().join("client/services/CheckoutService.kt"),
                service: String::from("checkout"),
                class: String::from("CheckoutService"),
                web_assembly: browser,
                application_provider: false,
            }],
            NativeCompanion::Kotlin,
        )
        .map(|registry| registry.expect("emitted Kotlin registry"))
    }
}

#[test]
fn a_kotlin_service_and_its_helper_create_the_helpers_they_need() {
    for browser in [true, false] {
        let registry = KotlinHelperProject::registry(KotlinHelperProject::TAX, browser)
            .expect("Kotlin Helpers");
        for expected in [
            "private val helper: TaxHelper = TaxHelper()",
            "private val rates: TaxRateHelper = TaxRateHelper()",
            "class TaxRateHelper {",
        ] {
            assert!(
                registry.contains(expected),
                "missing {expected}\n{registry}"
            );
        }
        assert!(!registry.contains("@Helper"), "{registry}");
        assert!(!registry.contains("constructor(rates"), "{registry}");
    }
}

#[test]
fn a_kotlin_helper_refuses_managed_lifetimes_and_hooks() {
    for (marker, member) in [
        ("@NeedsAll\n", ""),
        ("@Lifetime({\"value\":\"prototype\"})\n", ""),
        ("", "    @OnStart\n    fun open() {}\n"),
        ("", "    @OnEvent\n    fun heard(value: TaxRateHelper) {}\n"),
    ] {
        let tax = format!(
            "{marker}{}",
            KotlinHelperProject::TAX.replace("    fun addTax", &format!("{member}    fun addTax"))
        );
        let Err(error) = KotlinHelperProject::registry(&tax, true) else {
            panic!("refused {marker}{member}");
        };
        assert!(
            error
                .to_string()
                .contains("A Kotlin Tac Helper is a new object"),
            "{marker}{member}: {error}"
        );
    }
}
