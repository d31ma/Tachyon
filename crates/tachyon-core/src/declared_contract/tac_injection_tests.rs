use super::DeclaredContract;
use std::path::Path;

struct TacInjection;

impl TacInjection {
    const STORES: [&'static str; 1] = ["ProductStore"];
    const CONVERTERS: [&'static str; 1] = ["ProductConverter"];
    const BROWSER_STORES: [(&'static str, &'static str); 2] = [
        (
            "js",
            "@Store(ProductEntity, {\"setup\":\"CatalogSetup\"})\nexport default class ProductStore {\n  async findByStatus(status) {}\n}\n",
        ),
        (
            "ts",
            "@Store(ProductEntity, {\"setup\":\"CatalogSetup\"})\nexport default class ProductStore {\n  async findByStatus(status: string): Promise<ProductEntity[] | void> {}\n}\n",
        ),
    ];
    const NATIVE_SERVICES: [(&'static str, &'static str, &'static str); 7] = [
        (
            "kt",
            "@Service\nclass ProductService {\n    @Needs\n    val products: ProductStore\n    var summary: String = \"\"\n}\n",
            "    private val products: ProductStore = ProductStore()",
        ),
        (
            "swift",
            "@Service\nfinal class ProductService {\n    @Needs\n    let products: ProductStore\n    var summary: String = \"\"\n}\n",
            "    private let products: ProductStore = ProductStore()",
        ),
        (
            "java",
            "@Service\npublic class ProductService {\n    @Needs\n    ProductStore products;\n    public String summary = \"\";\n}\n",
            "    private final ProductStore products = new ProductStore();",
        ),
        (
            "cs",
            "[Service]\npublic class ProductService {\n    [Needs]\n    ProductStore _products;\n    public string summary = \"\";\n}\n",
            "    private readonly ProductStore _products = new ProductStore();",
        ),
        (
            "rs",
            "#[Service]\n#[derive(Default)]\nstruct ProductService {\n    #[Needs]\n    products: std::sync::Arc<ProductStore>,\n    summary: String,\n}\n",
            "struct ProductService {\n\n    products: std::sync::Arc<ProductStore>,",
        ),
        (
            "go",
            "// @Service\ntype ProductService struct {\n\t// @Needs\n\tproducts *ProductStore\n\tSummary string\n}\n",
            "\tproducts *ProductStore\n\tSummary string\n}\n\nfunc (service *ProductService) tachyonInject() error {\n\tproducts, err := NewProductStore()\n\tif err != nil {\n\t\treturn err\n\t}\n\tservice.products = products\n\treturn nil\n}\n",
        ),
        (
            "rb",
            "#[Service]\nclass ProductService\n  #[Needs]\n  # @return [ProductStore]\n  attr_reader :products\nend\n",
            "  private def products\n    @products ||= ProductStore.new\n  end",
        ),
    ];

    fn stores() -> Vec<String> {
        Self::names(&Self::STORES)
    }

    fn converters() -> Vec<String> {
        Self::names(&Self::CONVERTERS)
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn as_converter(text: &str) -> String {
        text.replace("ProductStore", "ProductConverter")
    }

    fn field_line(source: &str) -> Option<usize> {
        source.lines().position(|line| line.contains("roducts"))
    }

    fn service_path(extension: &str) -> String {
        format!("client/services/ProductService.{extension}")
    }
}

#[test]
fn a_browser_store_is_a_provider_that_opens_the_context_of_its_setup() {
    for (extension, source) in TacInjection::BROWSER_STORES {
        let path = format!("client/stores/ProductStore.{extension}");
        let lowered = DeclaredContract::concrete(Path::new(&path), source, "tac")
            .expect("the store is lowered");
        for part in [
            "static tachyonContext(signal",
            "return Behaviors.create(CatalogSetup, { partition: 'product', signal });",
        ] {
            assert!(
                lowered.contains(part),
                "{extension} lacks {part:?}:\n{lowered}"
            );
        }
        assert!(!lowered.contains("@Needs"), "{extension}:\n{lowered}");
    }
}

#[test]
fn a_native_service_field_that_needs_a_store_builds_the_store() {
    for (extension, source, filled) in TacInjection::NATIVE_SERVICES {
        let path = TacInjection::service_path(extension);
        let injected =
            DeclaredContract::injected(Path::new(&path), source, &TacInjection::stores())
                .unwrap_or_else(|| panic!("{extension} was not injected"));
        assert!(
            injected.contains(filled),
            "{extension} lacks {filled:?}:\n{injected}"
        );
        assert!(!injected.contains("Needs"), "{extension}:\n{injected}");
        assert_eq!(
            TacInjection::field_line(&injected),
            TacInjection::field_line(source),
            "{extension} keeps the line of the field:\n{injected}"
        );
    }
}

#[test]
fn a_native_service_field_that_needs_a_converter_builds_the_converter() {
    for (extension, source, filled) in TacInjection::NATIVE_SERVICES {
        let path = TacInjection::service_path(extension);
        let source = TacInjection::as_converter(source);
        let filled = TacInjection::as_converter(filled);
        let injected =
            DeclaredContract::injected(Path::new(&path), &source, &TacInjection::converters())
                .unwrap_or_else(|| panic!("{extension} was not injected"));
        assert!(
            injected.contains(&filled),
            "{extension} lacks {filled:?}:\n{injected}"
        );
        assert!(!injected.contains("Needs"), "{extension}:\n{injected}");
    }
}

#[test]
fn an_fsharp_constructor_parameter_typed_with_a_converter_becomes_a_converter_binding() {
    let source = "[<Service>]\ntype CatalogService(converter: ProductConverter) =\n    member val title : string = \"\" with get, set\n";
    let path = TacInjection::service_path("fs");
    let injected =
        DeclaredContract::injected(Path::new(&path), source, &TacInjection::converters())
            .expect("the F# service was not injected");
    assert!(
        injected.contains(
            "type CatalogService() =\n    let converter = ProductConverter()\n    member val title"
        ),
        "{injected}"
    );
}

#[test]
fn an_fsharp_constructor_parameter_typed_with_a_store_becomes_a_store_binding() {
    let source = "[<Service>]\ntype ProductService(prices: PriceClient, products: ProductStore) =\n    member val summary : string = \"\" with get, set\n";
    let path = TacInjection::service_path("fs");
    let injected = DeclaredContract::injected(Path::new(&path), source, &TacInjection::stores())
        .expect("the F# service was not injected");
    assert!(
        injected.contains(
            "type ProductService(prices: PriceClient) =\n    let products = ProductStore()\n    member val summary"
        ),
        "{injected}"
    );
    let other = "[<Service>]\ntype ProductService(prices: PriceClient) =\n    member this.Total() : int = 1\n";
    let left = DeclaredContract::injected(Path::new(&path), other, &TacInjection::stores());
    assert!(left.is_none(), "{left:?}");
}

#[test]
fn a_needs_field_of_another_type_is_left_alone() {
    let source = "@Service\nclass ProductService {\n    @Needs\n    val prices: PriceClient\n}\n";
    let path = TacInjection::service_path("kt");
    let injected = DeclaredContract::injected(Path::new(&path), source, &TacInjection::stores());
    assert!(injected.is_none(), "{injected:?}");
}

#[test]
fn only_stores_and_converters_are_injected_contracts() {
    let paths = [
        Path::new("client/stores/ProductStore.kt"),
        Path::new("client/entities/ProductEntity.kt"),
        Path::new("client/setups/CatalogSetup.kt"),
        Path::new("client/converters/ProductConverter.kt"),
    ];
    let names = DeclaredContract::injected_names(paths.into_iter());
    assert_eq!(
        names,
        [TacInjection::stores(), TacInjection::converters()].concat()
    );
}

#[test]
fn a_swift_service_field_that_needs_a_remote_builds_the_client() {
    let paths = [Path::new("client/remotes/GreetingClient.swift")];
    let names = DeclaredContract::injected_names(paths.into_iter());
    assert_eq!(names, ["GreetingClient"]);
    let source = "@Service\nfinal class GreetingService {\n    @Needs\n    private let greetingClient: GreetingClient\n}\n";
    let path = TacInjection::service_path("swift");
    let filled = DeclaredContract::injected(Path::new(&path), source, &names);
    let filled = filled.unwrap_or_default();
    assert!(
        filled.contains("    private let greetingClient: GreetingClient = GreetingClient()"),
        "{filled}"
    );
}

#[test]
fn a_swift_field_that_needs_a_helper_builds_it_and_a_rust_one_does_not() {
    let paths = [
        Path::new("client/helpers/cart/CartHelper.swift"),
        Path::new("client/helpers/CartHelper.rs"),
    ];
    let names = DeclaredContract::injected_names(paths.into_iter());
    assert_eq!(names, ["CartHelper"]);
    let source = "@Service\nfinal class CartService {\n    @Needs\n    private let cartHelper: CartHelper\n}\n";
    let path = TacInjection::service_path("swift");
    let filled = DeclaredContract::injected(Path::new(&path), source, &names).unwrap_or_default();
    assert!(
        filled.contains("    private let cartHelper: CartHelper = CartHelper()"),
        "{filled}"
    );
}

#[test]
fn a_java_service_field_that_needs_a_remote_builds_the_client() {
    let paths = [Path::new("client/remotes/GreetingClient.java")];
    let names = DeclaredContract::injected_names(paths.into_iter());
    assert_eq!(names, ["GreetingClient"]);
    let source = "@Service\nclass GreetingService {\n    @Needs\n    private final GreetingClient greetingClient;\n}\n";
    let path = TacInjection::service_path("java");
    let filled = DeclaredContract::injected(Path::new(&path), source, &names);
    let filled = filled.unwrap_or_default();
    assert!(
        filled.contains("    private final GreetingClient greetingClient = new GreetingClient();"),
        "{filled}"
    );
}

#[test]
fn a_ruby_service_field_that_needs_a_remote_builds_the_client_with_the_test_context() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    let files = [
        (
            "client/remotes/GreetingClient.rb",
            "#[HTTPClient(\"/api\")]\nclass GreetingClient\n  # @return [Hash]\n  #[GetRequest(\"/greeting\")]\n  def read\n  end\nend\n",
        ),
        (
            "client/setups/GreetingSetup.rb",
            "#[Setup]\nclass GreetingSetup\n  # @param execution [BehaviorExecution]\n  # @return [Hash]\n  def self.configure(execution)\n    {}\n  end\nend\n",
        ),
    ];
    for (relative, text) in files {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap_or(root.path()))
            .unwrap_or_else(|error| panic!("folder: {error}"));
        std::fs::write(path, text).unwrap_or_else(|error| panic!("source: {error}"));
    }
    let source = "#[Service]\nclass GreetingService\n  #[Needs]\n  # @return [GreetingClient]\n  attr_reader :greeting_client\n\n  def ask\n    greeting_client.read\n  end\nend\n";
    let service = root.path().join("client/services/GreetingService.rb");
    std::fs::create_dir_all(service.parent().unwrap_or(root.path()))
        .unwrap_or_else(|error| panic!("folder: {error}"));
    std::fs::write(service, source).unwrap_or_else(|error| panic!("service: {error}"));
    let assembled = crate::annotation_modules::assemble(
        root.path(),
        Path::new("client/services/GreetingService.rb"),
        source,
        "tac",
    )
    .unwrap_or_else(|failure| panic!("assembled: {failure}"))
    .unwrap_or_default();
    assert!(
        assembled.contains(
            "  private def greeting_client\n    @greeting_client ||= GreetingClient.new(@tachyon_test_context)\n  end"
        ),
        "{assembled}"
    );
}
