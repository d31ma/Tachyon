// @ts-check
import PlatformGenerator from '../platform-generator.js';
import NativeUIPlatformProject from '../../native-ui/platform-project.js';

/** Generates the native-first Jetpack Compose host for Android. */
export default class AndroidGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        await NativeUIPlatformProject.generate(this);
    }

    stringsXml() {
        return `<resources>\n    <string name="app_name">${this.appName}</string>\n</resources>\n`;
    }

    manifestXml() {
        return `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="${this.appId}">
    <uses-permission android:name="android.permission.INTERNET" />
    <application android:allowBackup="false" android:label="@string/app_name"
        android:icon="@mipmap/ic_launcher" android:roundIcon="@mipmap/ic_launcher_round"
        android:theme="@android:style/Theme.Material.Light.NoActionBar" android:usesCleartextTraffic="false">
        <activity android:name=".MainActivity" android:exported="true"
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
    androidResources { ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~" }
    defaultConfig {
        applicationId = "${this.appId}"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "${this.version}"
    }
    signingConfigs {
        create("release") {
            val keystorePath = System.getenv("TAC_ANDROID_KEYSTORE")
            if (keystorePath != null) {
                storeFile = file(keystorePath)
                storePassword = System.getenv("TAC_ANDROID_KEYSTORE_PASSWORD") ?: ""
                keyAlias = System.getenv("TAC_ANDROID_KEY_ALIAS") ?: ""
                keyPassword = System.getenv("TAC_ANDROID_KEY_PASSWORD") ?: (System.getenv("TAC_ANDROID_KEYSTORE_PASSWORD") ?: "")
            }
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (System.getenv("TAC_ANDROID_KEYSTORE") != null) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
}
dependencies {
    implementation("androidx.webkit:webkit:1.11.0")
}
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
        return `pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories { google(); mavenCentral() }
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
}
