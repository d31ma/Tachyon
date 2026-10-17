#![allow(clippy::expect_used)]

use std::path::Path;

fn lowered(fake: &str) -> String {
    let source = format!(
        "@Test(TitleTestSetup)\nclass TitleServiceTest {{\n {fake}\n fun testWrite() {{}}\n}}\n\
         @TestSetup\nclass TitleTestSetup {{\n companion object {{ fun configure(execution: Behaviors.Execution): Behaviors.Configuration = Behaviors.Configuration() }}\n}}\n\
         @Service\nclass TitleService {{\n @Needs\n val documentTitle: DocumentTitle\n fun write(title: String) {{ documentTitle.set(title) }}\n}}\n"
    );
    let path = Path::new("client/tests/TitleServiceTest.kt");
    let source = crate::behavior::ServiceRoles::read(path, &source)
        .expect("role stripping")
        .source;
    crate::behavior::lower(path, &source, "tac")
        .expect("Test lowering")
        .expect("source")
}

#[test]
fn kotlin_test_document_title_construct_uses_an_assigned_typed_fake() {
    let source = lowered("@Fake\n var documentTitle: DocumentTitle? = null");
    assert!(source.contains("return type.java.cast(TitleService(checkNotNull(this.documentTitle)"));
    assert!(source.contains("Test.construct needs an assigned Fake named documentTitle"));
    assert!(source.contains("Test.construct needs an active test setup"));
    assert!(!source.contains("BrowserDocumentTitle"));
}

#[test]
fn kotlin_test_document_title_construct_refuses_unmarked_and_wrong_typed_values() {
    for field in [
        "var documentTitle: DocumentTitle? = null",
        "@Fake\n var documentTitle: String? = null",
    ] {
        let source = lowered(field);
        assert!(source.contains("matching typed Fake named documentTitle for TitleService"));
        assert!(!source.contains("return type.java.cast(TitleService("));
    }
}

#[test]
fn every_kotlin_fake_is_cleared_before_each_case() {
    let source = lowered(
        "@Fake\n var documentTitle: DocumentTitle? = null\n @Fake\n var count: Int = 0\n @Fake\n var seen: Boolean = false",
    );
    assert!(
        source.contains(
            "for (name in listOf(\"testWrite\")) {\nthis.documentTitle = null\nthis.count = 0\nthis.seen = false\nstartBehaviors()"
        ),
        "{source}"
    );
}

#[test]
fn a_kotlin_fake_that_cannot_be_cleared_is_refused() {
    let source = "@Test(TitleTestSetup)\nclass TitleServiceTest {\n @Fake\n lateinit var documentTitle: DocumentTitle\n fun testWrite() {}\n}\n";
    let failure =
        crate::behavior::lower(Path::new("client/tests/TitleServiceTest.kt"), source, "tac")
            .expect_err("a Fake without an empty value");
    assert!(
        format!("{failure:?}").contains("Kotlin Fake documentTitle has the type DocumentTitle"),
        "{failure:?}"
    );
}
