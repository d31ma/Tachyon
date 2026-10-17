#![allow(clippy::expect_used)]

use std::path::Path;

struct KotlinHelperUnit;

impl KotlinHelperUnit {
    const FILES: [(&'static str, &'static str); 5] = [
        (
            "client/helpers/TaxRateHelper.kt",
            "@Helper\nclass TaxRateHelper {\n    fun rate(): Int = 2\n}\n",
        ),
        (
            "client/helpers/TaxHelper.kt",
            "@Helper\nclass TaxHelper {\n    @Needs\n    val rates: TaxRateHelper\n\n    fun addTax(subtotal: Int): Int = subtotal + rates.rate()\n}\n",
        ),
        (
            "client/services/CheckoutService.kt",
            "@Service\nclass CheckoutService {\n    @Needs\n    val helper: TaxHelper\n    var total: Int = 0\n\n    fun checkout() {\n        total = helper.addTax(40)\n    }\n}\n",
        ),
        (
            "client/tests/CheckoutTestSetup.kt",
            "@TestSetup\nclass CheckoutTestSetup {\n    companion object {\n        fun configure(execution: Behaviors.Execution): Behaviors.Configuration = Behaviors.Configuration()\n    }\n}\n",
        ),
        (
            "client/tests/CheckoutServiceTest.kt",
            "@Test(CheckoutTestSetup::class, {\"partition\":\"tests\"})\nclass CheckoutServiceTest {\n    @Fake\n    var helper: TaxHelper? = null\n\n    \
             fun testCheckoutAddsTheTax() {\n        helper = TaxHelper()\n        val checkout = construct(CheckoutService::class)\n        checkout.checkout()\n        Assert.equals(42, checkout.total)\n    }\n}\n",
        ),
    ];

    fn write(root: &Path) {
        for (relative, source) in Self::FILES {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("fixture parent"))
                .expect("fixture directory");
            std::fs::write(path, source).expect("fixture source");
        }
    }
}

#[test]
fn a_kotlin_test_unit_builds_a_service_with_its_helpers() {
    if std::process::Command::new("kotlinc")
        .arg("-version")
        .output()
        .is_err()
    {
        assert!(std::env::var_os("CI").is_none(), "CI must provide kotlinc");
        eprintln!("skipping: kotlinc is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("test project");
    KotlinHelperUnit::write(root.path());
    let report = super::run(
        root.path(),
        Path::new("client/tests/CheckoutServiceTest.kt"),
    )
    .expect("native Kotlin test unit");
    assert_eq!(report.cases.len(), 1, "{report:?}");
    assert!(report.cases.iter().all(|case| case.passed), "{report:?}");
}
