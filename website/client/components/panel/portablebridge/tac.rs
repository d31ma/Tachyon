struct PortableBridge {
    web_status: String,
    stored_value: String,
    device_status: String,
    mobile_status: String,
    file_status: String,
    clipboard_status: String,
    permission_status: String,
    security_status: String,
}

impl PortableBridge {
    fn new() -> Self {
        Self {
            web_status: "checking browser",
            stored_value: "not read",
            device_status: "request app().info() to inspect the target",
            mobile_status: "share and haptics await native-tree adapters",
            file_status: "native filesystem has not been inspected",
            clipboard_status: "clipboard bridge has not been requested",
            permission_status: "inspect location permission state",
            security_status: "secure storage and user verification are host-gated",
        }
    }

    #[onMount]
    fn inspect_web(&mut self) {
        local_storage().set_item("tachyon.portable.bridge", "saved by tac.rs");
        fylo().collection("tac-companion-probes").find({});
        self.stored_value = local_storage().get_item("tachyon.portable.bridge", "storage unavailable");
        self.web_status = navigator().language();
    }

    async fn inspect_device(&mut self) {
        if !app().is_available() {
            self.device_status = "app metadata is unavailable on this target";
            return;
        }
        self.device_status = "requesting app.info";
        let info = await app().info();
        self.device_status = info.name;
    }

    async fn test_mobile(&mut self) {
        if !capabilities().supports("share.text") || !capabilities().supports("haptics.impact") {
            self.mobile_status = "unavailable until native-tree adapters are installed";
            return;
        }
        await share().text("Tachyon companion mobile bridge");
        await haptics().impact();
        self.mobile_status = "native share sheet and haptic feedback requested";
    }

    async fn inspect_filesystem(&mut self) {
        if !capabilities().supports("fs.readDir") {
            self.file_status = "native filesystem is unavailable in this browser build";
            return;
        }
        await file_system().read_dir(".");
        self.file_status = "native filesystem request completed";
    }

    async fn copy_report(&mut self) {
        if !capabilities().supports("clipboard.writeText") {
            self.clipboard_status = "clipboard requires a clipboard.writeText bundle capability";
            return;
        }
        await clipboard().write_text("Tachyon portable bridge report");
        self.clipboard_status = "bridge report copied through the native clipboard";
    }

    async fn inspect_permissions(&mut self) {
        self.permission_status = await capabilities().state("geo.current");
        if capabilities().supports("secrets.get") && capabilities().supports("auth.verifyUser") {
            self.security_status = "this host provides secure storage and local user verification";
        } else {
            self.security_status = "unavailable here by design; secure APIs fail closed outside supported hosts";
        }
    }
}
