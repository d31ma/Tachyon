#![allow(clippy::expect_used)]

use super::swift_tests::swift_web_ready;
use super::{BuildJobs, WebLanguage, build_all};
use std::path::Path;
use std::process::Command;

struct SwiftHelpers;

impl SwiftHelpers {
    const FILES: [(&str, &str); 3] = [
        (
            "client/helpers/CartPricesHelper.swift",
            "@Helper\nfinal class CartPricesHelper {\n    var served: Int = 0\n\n    func subtotal() -> Int {\n        served += 1\n        return 40\n    }\n\n    func surcharge() -> Int { 2 }\n}\n",
        ),
        (
            "client/helpers/cart/CartHelper.swift",
            "@Helper\nfinal class CartHelper {\n    @Needs\n    private let cartPricesHelper: CartPricesHelper\n\n    func settle(_ subtotal: Int) -> Int {\n        subtotal + cartPricesHelper.surcharge()\n    }\n}\n",
        ),
        (
            "client/services/CartService.swift",
            "@Service\n@Lifetime({\"value\":\"prototype\"})\nfinal class CartService {\n    @Needs\n    private let cartPricesHelper: CartPricesHelper\n    @Needs\n    private let cartHelper: CartHelper\n\n    func total() -> Int {\n        cartHelper.settle(cartPricesHelper.subtotal())\n    }\n\n    func served() -> Int { cartPricesHelper.served }\n}\n",
        ),
    ];

    const DRIVER: &str = "import { readFile } from 'node:fs/promises'\n\
         globalThis.fetch = async (url) => new Response(await readFile(url))\n\
         const { default: CartService } = await import('./CartService.js')\n\
         const one = new CartService({}), two = new CartService({})\n\
         one.__tachyonStarted(); two.__tachyonStarted()\n\
         const total = one.total()\n\
         one.total(); two.total()\n\
         const served = [one.served(), two.served()]\n\
         one.__tachyonDispose()\n\
         const fresh = new CartService({})\n\
         fresh.__tachyonStarted()\n\
         console.log(JSON.stringify({ total, served, fresh: fresh.served() }))\n\
         process.exit(0)\n";

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(&path, text).expect("file");
    }

    async fn publish(root: &Path) {
        Self::built(root)
            .await
            .unwrap_or_else(|failure| panic!("{failure}"));
    }

    async fn built(root: &Path) -> Result<(), crate::Failure> {
        for (relative, text) in Self::FILES {
            if !root.join(relative).exists() {
                Self::write(root, relative, text);
            }
        }
        let services = [(
            String::from("cart"),
            String::from("CartService"),
            WebLanguage::Swift,
            root.join("client/services/CartService.swift"),
        )];
        let built = build_all(
            root,
            &services,
            &BuildJobs::new(1),
            crate::BuildProfile::Development,
        )
        .await?;
        for (name, bytes) in built.files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
            std::fs::write(path, bytes).expect("publish");
        }
        Ok(())
    }

    fn drive(root: &Path) -> serde_json::Value {
        std::fs::write(root.join("drive.mjs"), Self::DRIVER).expect("driver");
        let output = Command::new("node")
            .arg("drive.mjs")
            .current_dir(root)
            .output()
            .expect("node");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(stdout.trim()).expect("report")
    }
}

#[tokio::test]
async fn a_swift_service_owns_the_helpers_it_needs() {
    if !swift_web_ready() {
        eprintln!("skipping: the Swift SDK for WebAssembly, swiftc or node is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    SwiftHelpers::publish(root.path()).await;
    let report = SwiftHelpers::drive(root.path());
    assert_eq!(
        report,
        serde_json::json!({ "total": 42, "served": [2, 1], "fresh": 0 }),
        "each service builds its own helpers, and a helper builds the helpers it needs"
    );
}

#[tokio::test]
async fn a_swift_helper_has_no_lifecycle_hooks() {
    let root = tempfile::tempdir().expect("project");
    SwiftHelpers::write(
        root.path(),
        "client/helpers/CartPricesHelper.swift",
        "@Helper\nfinal class CartPricesHelper {\n    var served: Int = 0\n\n    @OnStart\n    func start() {\n        served = 1\n    }\n\n    func subtotal() -> Int { 40 }\n\n    func surcharge() -> Int { 2 }\n}\n",
    );
    let Err(failure) = SwiftHelpers::built(root.path()).await else {
        panic!("a Swift Helper hook is refused");
    };
    assert!(failure.to_string().contains("TY1405"), "{failure}");
}
