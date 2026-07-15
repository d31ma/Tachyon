// @ts-check
import path from 'path';
import { mkdir, writeFile } from 'fs/promises';
import PlatformGenerator from '../platform-generator.js';

/**
 * Generates an Android host project scaffold using the native WebView.
 *
 * Output layout:
 *   <outputRoot>/
 *     Resources/                 # copied Tac assets
 *     app/
 *       src/main/java/ma/del/tachyon/
 *         MainActivity.kt
 *       src/main/res/values/
 *         strings.xml
 *       src/main/
 *         AndroidManifest.xml
 *       build.gradle.kts
 *     build.gradle.kts
 *     settings.gradle.kts
 *     gradle.properties
 *     README.md
 *     tachyon.host.json
 */
export default class AndroidGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        const javaPackageDir = path.join(this.outputRoot, 'app', 'src', 'main', 'java', ...this.appId.split('.'));
        const resValuesDir = path.join(this.outputRoot, 'app', 'src', 'main', 'res', 'values');

        await mkdir(javaPackageDir, { recursive: true });
        await mkdir(resValuesDir, { recursive: true });

        await writeFile(path.join(javaPackageDir, 'MainActivity.kt'), this.mainActivity());
        await writeFile(path.join(resValuesDir, 'strings.xml'), this.stringsXml());
        await writeFile(path.join(this.outputRoot, 'app', 'src', 'main', 'AndroidManifest.xml'), this.manifestXml());
        await writeFile(path.join(this.outputRoot, 'app', 'build.gradle.kts'), this.appBuildGradle());
        await writeFile(path.join(this.outputRoot, 'build.gradle.kts'), this.rootBuildGradle());
        await writeFile(path.join(this.outputRoot, 'settings.gradle.kts'), this.settingsGradle());
        await writeFile(path.join(this.outputRoot, 'gradle.properties'), this.gradleProperties());
    }

    mainActivity() {
        const bridgeScript = this.getBridgeScript().replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
        const allowedCapabilities = this.nativeCapabilities.map(({ capability }) => JSON.stringify(capability)).join(', ');
        return `package ${this.appId}

import android.app.Activity
import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Color
import android.net.Uri
import android.content.pm.PackageManager
import android.webkit.GeolocationPermissions
import android.os.Bundle
import android.view.View
import android.view.HapticFeedbackConstants
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.ServiceWorkerClientCompat
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.ServiceWorkerControllerCompat
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class MainActivity : Activity() {
    companion object {
        // Latest safe-area CSS injection; replayed after every page load
        // because in-page style properties do not survive navigation.
        @Volatile var safeAreaScript: String = ""
    }

    private lateinit var deviceWebChromeClient: DeviceWebChromeClient

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val webView = WebView(this)
        // The page is the app: no title bar (see the manifest theme) and no
        // Android overlay scrollbars, so it feels native rather than embedded.
        webView.isVerticalScrollBarEnabled = false
        webView.isHorizontalScrollBarEnabled = false
        setContentView(webView)

        // Draw behind a transparent status bar so the page's own chrome fills
        // the whole screen — no opaque system strip above the app. Icons
        // default to dark-on-light; pages flip them through the
        // "ui.statusBarStyle" bridge capability when their theme changes.
        // (systemUiVisibility is deprecated but is the one API that covers
        // minSdk 26 through current releases.)
        window.statusBarColor = Color.TRANSPARENT
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR

        // WebView pages cannot read the status-bar inset from
        // env(safe-area-inset-top), so surface it as a CSS variable instead.
        webView.setOnApplyWindowInsetsListener { view, insets ->
            @Suppress("DEPRECATION")
            val top = insets.systemWindowInsetTop / resources.displayMetrics.density
            val script = "document.documentElement.style.setProperty('--tac-safe-top','" + top + "px')"
            safeAreaScript = script
            view.post { webView.evaluateJavascript(script, null) }
            insets
        }

        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
        }

        // Serve the bundled assets from WebViewAssetLoader's trusted https
        // origin instead of file://. A real (non-opaque) origin is what makes
        // OPFS (the FYLO browser mirror) and service workers available inside
        // the host — and it resolves the
        // bundle's absolute asset paths (/shared/...) naturally.
        // The domain is set explicitly: relying on the library default has
        // bitten us (resolved builds have shipped a different default), and
        // the loadUrl below must agree with it exactly. The handler serves
        // directory indexes so the app lives at "/" like the web build, and
        // deep links (/atlas) fall back to their prerendered index.html.
        val assetsHandler = WebViewAssetLoader.AssetsPathHandler(this)
        val indexAwareHandler = WebViewAssetLoader.PathHandler { path ->
            val primary = if (path.isEmpty() || path.endsWith("/")) path + "index.html" else path
            assetsHandler.handle(primary)
                ?: if (!path.contains('.')) assetsHandler.handle(path + "/index.html") else null
        }
        val assetLoader = WebViewAssetLoader.Builder()
            .setDomain("appassets.androidapp.com")
            .addPathHandler("/", indexAwareHandler)
            .build()

        // Service-worker-originated fetches bypass the WebViewClient, so wire
        // the same loader into the service worker controller when supported.
        if (WebViewFeature.isFeatureSupported(WebViewFeature.SERVICE_WORKER_BASIC_USAGE)) {
            ServiceWorkerControllerCompat.getInstance().setServiceWorkerClient(
                object : ServiceWorkerClientCompat() {
                    override fun shouldInterceptRequest(request: WebResourceRequest): WebResourceResponse? {
                        return assetLoader.shouldInterceptRequest(request.url)
                    }
                }
            )
        }

        deviceWebChromeClient = DeviceWebChromeClient(this)
        webView.webChromeClient = deviceWebChromeClient
        webView.webViewClient = TachyonWebViewClient(assetLoader, "${bridgeScript}")
        val nativeBridge = NativeBridge(webView)
        WebViewCompat.addWebMessageListener(
            webView,
            "__tcNativeHost__",
            setOf("https://appassets.androidapp.com"),
            object : WebViewCompat.WebMessageListener {
                override fun onPostMessage(
                    view: WebView,
                    message: WebMessageCompat,
                    sourceOrigin: Uri,
                    isMainFrame: Boolean,
                    replyProxy: JavaScriptReplyProxy,
                ) {
                    if (!isMainFrame || sourceOrigin.scheme != "https" || sourceOrigin.host != "appassets.androidapp.com") return
                    nativeBridge.postMessage(message.data ?: "")
                }
            },
        )

        webView.loadUrl("https://appassets.androidapp.com/")
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        deviceWebChromeClient.onDevicePermissionResult(requestCode, grantResults.all { it == PackageManager.PERMISSION_GRANTED })
    }
}

class DeviceWebChromeClient(private val activity: Activity) : WebChromeClient() {
    companion object {
        const val MEDIA_PERMISSION_REQUEST = 731
        const val LOCATION_PERMISSION_REQUEST = 732
    }

    private var pendingMediaRequest: Pair<PermissionRequest, Array<String>>? = null
    private var pendingLocationRequest: Pair<String, GeolocationPermissions.Callback>? = null

    override fun onPermissionRequest(request: PermissionRequest) {
        val permissions = mutableListOf<String>()
        val allowedResources = request.resources.filter {
            it == PermissionRequest.RESOURCE_VIDEO_CAPTURE || it == PermissionRequest.RESOURCE_AUDIO_CAPTURE
        }.toTypedArray()
        if (allowedResources.contains(PermissionRequest.RESOURCE_VIDEO_CAPTURE)) permissions.add(Manifest.permission.CAMERA)
        if (allowedResources.contains(PermissionRequest.RESOURCE_AUDIO_CAPTURE)) permissions.add(Manifest.permission.RECORD_AUDIO)
        if (permissions.isEmpty()) { request.deny(); return }
        if (permissions.all { activity.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }) {
            request.grant(allowedResources)
            return
        }
        pendingMediaRequest?.first?.deny()
        pendingMediaRequest = request to allowedResources
        activity.requestPermissions(permissions.toTypedArray(), MEDIA_PERMISSION_REQUEST)
    }

    override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) {
        if (activity.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED) {
            callback.invoke(origin, true, false)
            return
        }
        pendingLocationRequest?.second?.invoke(pendingLocationRequest!!.first, false, false)
        pendingLocationRequest = origin to callback
        activity.requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION), LOCATION_PERMISSION_REQUEST)
    }

    fun onDevicePermissionResult(requestCode: Int, granted: Boolean) {
        when (requestCode) {
            MEDIA_PERMISSION_REQUEST -> {
                val request = pendingMediaRequest
                pendingMediaRequest = null
                if (granted) request?.first?.grant(request.second) else request?.first?.deny()
            }
            LOCATION_PERMISSION_REQUEST -> {
                val request = pendingLocationRequest
                pendingLocationRequest = null
                request?.second?.invoke(request.first, granted, false)
            }
        }
    }
}

class TachyonWebViewClient(
    private val assetLoader: WebViewAssetLoader,
    private val bridgeScript: String,
) : WebViewClient() {
    override fun onPageFinished(view: WebView, url: String) {
        super.onPageFinished(view, url)
        view.evaluateJavascript(bridgeScript, null)
        if (MainActivity.safeAreaScript.isNotEmpty())
            view.evaluateJavascript(MainActivity.safeAreaScript, null)
    }

    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        return assetLoader.shouldInterceptRequest(request.url)
    }

    // The app's own origin stays in the WebView; external links (GitHub,
    // docs, mailto) open in the user's default browser — navigating the
    // WebView away would strand them with no way back into the app.
    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        val url = request.url
        if (url.host == "appassets.androidapp.com") return false
        if (url.scheme != "http" && url.scheme != "https") return true
        return try {
            view.context.startActivity(Intent(Intent.ACTION_VIEW, url))
            true
        } catch (error: Throwable) {
            false
        }
    }
}

class NativeBridge(private val webView: WebView) {
    private val allowedCapabilities = setOf(${allowedCapabilities})

    fun postMessage(message: String) {
        var requestId = 0
        try {
            val envelope = JSONObject(message)
            if (envelope.optString("type") != "tac:native-request") {
                sendResponse(errorResponse(0, "Invalid native request envelope"))
                return
            }
            requestId = envelope.optInt("id", 0)
            val capability = envelope.optString("capability")
            val payload = envelope.optJSONObject("payload") ?: JSONObject()
            if (!allowedCapabilities.contains(capability)) {
                sendResponse(errorResponse(requestId, "Native capability is not enabled: $capability"))
                return
            }
            // User verification is asynchronous (it shows a system prompt), so
            // it completes the response from its own callback rather than the
            // synchronous handle() path.
            if (capability == "auth.verifyUser") {
                verifyUser(requestId, payload.optString("reason", "Verify your identity"))
                return
            }
            sendResponse(successResponse(requestId, handle(capability, payload)))
        } catch (error: Throwable) {
            sendResponse(errorResponse(requestId, error.message ?: "Native capability failed"))
        }
    }

    private fun sendResponse(response: JSONObject) {
        val script = "if(window.__tcNativeBridge__.messageHandler)window.__tcNativeBridge__.messageHandler(" + JSONObject.quote(response.toString()) + ")"
        webView.post {
            webView.evaluateJavascript(script, null)
        }
    }

    private fun verifyUser(id: Int, reason: String) {
        val activity = webView.context as? Activity
        if (activity == null) {
            sendResponse(errorResponse(id, "User verification requires an Activity context"))
            return
        }
        if (android.os.Build.VERSION.SDK_INT < 28) {
            sendResponse(errorResponse(id, "User verification requires Android 9 (API 28) or newer"))
            return
        }
        val builder = android.hardware.biometrics.BiometricPrompt.Builder(activity)
            .setTitle("Verify it's you")
            .setDescription(reason)
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            builder.setAllowedAuthenticators(
                android.hardware.biometrics.BiometricManager.Authenticators.BIOMETRIC_STRONG or
                    android.hardware.biometrics.BiometricManager.Authenticators.DEVICE_CREDENTIAL
            )
        } else {
            @Suppress("DEPRECATION")
            builder.setDeviceCredentialAllowed(true)
        }
        builder.build().authenticate(
            android.os.CancellationSignal(),
            activity.mainExecutor,
            object : android.hardware.biometrics.BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: android.hardware.biometrics.BiometricPrompt.AuthenticationResult) {
                    sendResponse(successResponse(id, JSONObject().put("verified", true)))
                }
                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    sendResponse(errorResponse(id, errString.toString()))
                }
                // A single non-match keeps the prompt open; only a terminal
                // error or success completes the request.
                override fun onAuthenticationFailed() {}
            }
        )
    }

    private fun handle(capability: String, payload: JSONObject): Any {
        return when (capability) {
            "app.info" -> JSONObject()
                .put("name", "${this.appName}")
                .put("runtime", "android-webview")
                .put("package", webView.context.packageName)
            "clipboard.readText" -> {
                val clipboard = webView.context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.primaryClip?.takeIf { it.itemCount > 0 }
                    ?.getItemAt(0)?.coerceToText(webView.context)?.toString() ?: ""
            }
            "clipboard.writeText" -> {
                val text = payload.optString("text", "")
                val clipboard = webView.context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(ClipData.newPlainText("Tachyon", text))
                JSONObject().put("written", true)
            }
            "openUrl" -> {
                val url = requireString(payload, "url")
                val uri = android.net.Uri.parse(url)
                if (uri.scheme != "http" && uri.scheme != "https") throw IllegalArgumentException("openUrl requires an http(s) URL")
                webView.context.startActivity(Intent(Intent.ACTION_VIEW, uri))
                JSONObject().put("opened", true)
            }
            "share.text" -> {
                val send = Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_TEXT, payload.optString("text", ""))
                    putExtra(Intent.EXTRA_TITLE, payload.optString("title", ""))
                }
                webView.context.startActivity(Intent.createChooser(send, payload.optString("title", "Share")))
                JSONObject().put("shared", true)
            }
            "haptics.impact" -> {
                webView.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                JSONObject().put("impacted", true)
            }
            "fs.readText" -> {
                val path = requireString(payload, "path")
                JSONObject().put("path", path).put("text", File(path).readText())
            }
            "fs.writeText" -> {
                val path = requireString(payload, "path")
                val text = payload.optString("text", "")
                File(path).writeText(text)
                JSONObject().put("path", path).put("bytes", text.toByteArray(Charsets.UTF_8).size).put("written", true)
            }
            "fs.readDir" -> {
                val path = requireString(payload, "path")
                val entries = JSONArray()
                File(path).listFiles()?.forEach { child ->
                    entries.put(JSONObject()
                        .put("name", child.name)
                        .put("type", if (child.isDirectory) "directory" else "file"))
                }
                JSONObject().put("path", path).put("entries", entries)
            }
            "fs.stat" -> {
                val file = File(requireString(payload, "path"))
                val result = JSONObject().put("path", file.path).put("exists", file.exists())
                if (file.exists()) result.put("type", if (file.isDirectory) "directory" else "file").put("size", file.length())
                result
            }
            "fs.mkdir" -> {
                val file = File(requireString(payload, "path"))
                if (!file.exists() && !file.mkdirs()) throw IllegalStateException("Unable to create directory: " + file.path)
                JSONObject().put("path", file.path).put("created", true)
            }
            "fs.remove" -> {
                val file = File(requireString(payload, "path"))
                if (file.exists() && !file.deleteRecursively()) throw IllegalStateException("Unable to remove path: " + file.path)
                JSONObject().put("path", file.path).put("removed", true)
            }
            "fs.paths" -> JSONObject()
                .put("appData", webView.context.filesDir.absolutePath)
                .put("cache", webView.context.cacheDir.absolutePath)
                .put("documents", webView.context.getExternalFilesDir(null)?.absolutePath ?: webView.context.filesDir.absolutePath)
            "ui.statusBarStyle" -> {
                // "light-content" = light icons for dark pages,
                // "dark-content" = dark icons for light pages.
                val style = requireString(payload, "style")
                val activity = webView.context as? Activity
                    ?: throw IllegalStateException("ui.statusBarStyle requires an Activity context")
                webView.post {
                    @Suppress("DEPRECATION")
                    val flags = activity.window.decorView.systemUiVisibility
                    @Suppress("DEPRECATION")
                    activity.window.decorView.systemUiVisibility = if (style == "light-content")
                        flags and View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR.inv()
                    else
                        flags or View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                }
                JSONObject().put("style", style)
            }
            "secrets.get" -> readSecret(requireString(payload, "key")) ?: JSONObject.NULL
            "secrets.set" -> {
                writeSecret(requireString(payload, "key"), requireString(payload, "value"))
                JSONObject().put("stored", true)
            }
            "secrets.delete" -> {
                secretPrefs().edit().remove(requireString(payload, "key")).apply()
                JSONObject().put("deleted", true)
            }
            "shell.exec" -> throw IllegalStateException("shell.exec is not available on Android native hosts")
            else -> throw IllegalStateException("Unsupported native capability: $capability")
        }
    }

    private fun requireString(payload: JSONObject, key: String): String {
        val value = payload.optString(key, "")
        if (value.isEmpty()) throw IllegalArgumentException("Native capability payload requires non-empty string: $key")
        return value
    }

    // Secure storage: values are encrypted with an AES/GCM key held in the
    // hardware-backed Android Keystore and persisted (ciphertext only) in a
    // private SharedPreferences file. Mirrors the iOS Keychain contract.
    private fun secretPrefs(): SharedPreferences =
        webView.context.getSharedPreferences("tac.secrets", Context.MODE_PRIVATE)

    private fun secretKey(): javax.crypto.SecretKey {
        val keyStore = java.security.KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey("tac.secrets.key", null) as? javax.crypto.SecretKey)?.let { return it }
        val generator = javax.crypto.KeyGenerator.getInstance(
            android.security.keystore.KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore"
        )
        generator.init(
            android.security.keystore.KeyGenParameterSpec.Builder(
                "tac.secrets.key",
                android.security.keystore.KeyProperties.PURPOSE_ENCRYPT or android.security.keystore.KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(android.security.keystore.KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(android.security.keystore.KeyProperties.ENCRYPTION_PADDING_NONE)
                .build()
        )
        return generator.generateKey()
    }

    private fun writeSecret(key: String, value: String) {
        val cipher = javax.crypto.Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(javax.crypto.Cipher.ENCRYPT_MODE, secretKey())
        val encrypted = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val encoded = android.util.Base64.encodeToString(cipher.iv, android.util.Base64.NO_WRAP) +
            ":" + android.util.Base64.encodeToString(encrypted, android.util.Base64.NO_WRAP)
        secretPrefs().edit().putString(key, encoded).apply()
    }

    private fun readSecret(key: String): String? {
        val stored = secretPrefs().getString(key, null) ?: return null
        val parts = stored.split(":")
        if (parts.size != 2) return null
        val iv = android.util.Base64.decode(parts[0], android.util.Base64.NO_WRAP)
        val encrypted = android.util.Base64.decode(parts[1], android.util.Base64.NO_WRAP)
        val cipher = javax.crypto.Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(javax.crypto.Cipher.DECRYPT_MODE, secretKey(), javax.crypto.spec.GCMParameterSpec(128, iv))
        return String(cipher.doFinal(encrypted), Charsets.UTF_8)
    }

    private fun successResponse(id: Int, value: Any): JSONObject {
        return JSONObject()
            .put("type", "tac:native-response")
            .put("id", id)
            .put("ok", true)
            .put("value", value)
    }

    private fun errorResponse(id: Int, message: String): JSONObject {
        return JSONObject()
            .put("type", "tac:native-response")
            .put("id", id)
            .put("ok", false)
            .put("error", message)
    }
}
`;
    }

    stringsXml() {
        return `<resources>
    <string name="app_name">${this.appName}</string>
</resources>
`;
    }

    manifestXml() {
        const permissions = [
            '<uses-permission android:name="android.permission.INTERNET" />',
            // Secure storage + user verification ship on every Android host, so
            // the biometric/device-credential permission is always declared.
            '<uses-permission android:name="android.permission.USE_BIOMETRIC" />',
            this.requestedDevicePermissions.has('camera') ? '<uses-permission android:name="android.permission.CAMERA" />' : '',
            this.requestedDevicePermissions.has('microphone') ? '<uses-permission android:name="android.permission.RECORD_AUDIO" />' : '',
            this.requestedDevicePermissions.has('location') ? '<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />' : '',
            this.requestedDevicePermissions.has('notifications') ? '<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />' : '',
        ].filter(Boolean).join('\n    ');
        return `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="${this.appId}">

    ${permissions}

    <application
        android:allowBackup="false"
        android:label="@string/app_name"
        android:icon="@mipmap/ic_launcher"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:theme="@android:style/Theme.Material.Light.NoActionBar"
        android:usesCleartextTraffic="false">
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|keyboardHidden">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
`;
    }

    appBuildGradle() {
        return `plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "${this.appId}"
    compileSdk = 34

    // AAPT's default pattern drops directories starting with "_" from assets,
    // which would strip Tachyon's on-disk dynamic-route modules (pages/_slug/).
    androidResources {
        ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~"
    }

    defaultConfig {
        applicationId = "${this.appId}"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "${this.version}"
    }

    signingConfigs {
        // Release signing comes from TAC_ANDROID_KEYSTORE(_PASSWORD)/
        // TAC_ANDROID_KEY_ALIAS(_PASSWORD) when provided.
        create("release") {
            val keystorePath = System.getenv("TAC_ANDROID_KEYSTORE")
            if (keystorePath != null) {
                storeFile = file(keystorePath)
                storePassword = System.getenv("TAC_ANDROID_KEYSTORE_PASSWORD") ?: ""
                keyAlias = System.getenv("TAC_ANDROID_KEY_ALIAS") ?: ""
                keyPassword = System.getenv("TAC_ANDROID_KEY_PASSWORD")
                    ?: (System.getenv("TAC_ANDROID_KEYSTORE_PASSWORD") ?: "")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            // Fall back to the debug keystore so exported release builds are
            // installable out of the box; provide TAC_ANDROID_KEYSTORE for
            // store distribution.
            signingConfig = if (System.getenv("TAC_ANDROID_KEYSTORE") != null)
                signingConfigs.getByName("release")
            else
                signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.webkit:webkit:1.11.0")
}

// Copy Tac assets into android assets folder at build time
android.sourceSets["main"].assets.srcDir("$rootDir/Resources")
`;
    }

    rootBuildGradle() {
        return `plugins {
    id("com.android.application") version "8.2.0" apply false
    id("org.jetbrains.kotlin.android") version "1.9.20" apply false
}
`;
    }

    settingsGradle() {
        return `pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "${this.appName}"
include(":app")
`;
    }

    gradleProperties() {
        return `org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
android.nonTransitiveRClass=true
`;
    }

    buildReadme() {
        return `# ${this.appName} — Android native host

This folder contains an Android WebView host scaffold for the Tac frontend.

## Prerequisites

- Android Studio (Iguana or newer)
- Android SDK 34
- JDK 17

## Build

1. Open this folder in **Android Studio**.
2. Let Gradle sync.
3. Choose **Run → Run 'app'**.

The \`Resources/\` folder is wired as an Android assets source set and served
through \`WebViewAssetLoader\` at the trusted origin
\`https://appassets.androidapp.com/\`, so absolute asset paths, Web Workers,
OPFS storage, and service workers all behave like the web build.

## Architecture

- Static Tac assets live in \`Resources/\`.
- \`MainActivity.kt\` creates an Android \`WebView\` and loads
  \`https://appassets.androidapp.com/index.html\` via \`WebViewAssetLoader\`.
- \`window.__tcNativeBridge__\` exposes a minimal JS↔native message contract.
`;
    }
}
