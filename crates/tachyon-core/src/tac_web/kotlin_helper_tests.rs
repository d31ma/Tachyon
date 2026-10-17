#![allow(clippy::expect_used)]

use super::kotlin_tests::kotlin_web_ready;
use super::{BuildJobs, WebLanguage, build_all};
use std::path::{Path, PathBuf};
use std::process::Command;

struct KotlinHelpers;

impl KotlinHelpers {
    const FILES: [(&str, &str); 3] = [
        (
            "client/helpers/TaxRateHelper.kt",
            "@Helper\nclass TaxRateHelper {\n    fun rate(): Int = 2\n}\n",
        ),
        (
            "client/helpers/TaxHelper.kt",
            "@Helper\nclass TaxHelper {\n    @Needs\n    val rates: TaxRateHelper\n\n    fun addTax(subtotal: Int): Int {\n        val tax = rates.rate()\n        return subtotal + tax\n    }\n}\n",
        ),
        (
            "client/services/CheckoutService.kt",
            "@Service\nclass CheckoutService {\n    @Needs\n    val helper: TaxHelper\n    var total: Int = 0\n\n    fun checkout(): Int {\n        total = helper.addTax(40)\n        return total\n    }\n}\n",
        ),
    ];

    const DRIVER: &str = "const { default: CheckoutService } = await import('./CheckoutService.js')\n\
         const checkout = new CheckoutService({})\n\
         console.log(JSON.stringify({ answer: checkout.checkout(), total: checkout.total }))\n\
         process.exit(0)\n";

    fn write(root: &Path, relative: &str, text: &str) -> PathBuf {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(&path, text).expect("file");
        path
    }
}

#[tokio::test]
async fn a_kotlin_service_needs_a_helper_that_needs_another_helper() {
    if !kotlin_web_ready() {
        eprintln!("skipping: kotlinc-js, TAC_KOTLIN_WASM_STDLIB or node is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    for (relative, text) in KotlinHelpers::FILES {
        KotlinHelpers::write(root.path(), relative, text);
    }
    let services = [(
        String::from("checkout"),
        String::from("CheckoutService"),
        WebLanguage::Kotlin,
        root.path().join("client/services/CheckoutService.kt"),
    )];
    let built = build_all(
        root.path(),
        &services,
        &BuildJobs::new(1),
        crate::BuildProfile::Development,
    )
    .await
    .unwrap_or_else(|failure| panic!("{failure}"));
    for (name, bytes) in built.files {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(path, bytes).expect("publish");
    }
    KotlinHelpers::write(root.path(), "drive.mjs", KotlinHelpers::DRIVER);
    let output = Command::new("node")
        .arg("drive.mjs")
        .current_dir(root.path())
        .output()
        .expect("node");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_str(stdout.trim()).expect("report");
    assert_eq!(report, serde_json::json!({"answer": 42, "total": 42}));
}
