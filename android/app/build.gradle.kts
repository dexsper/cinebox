plugins {
    alias(libs.plugins.android.application)
}

/** `version = "x.y.z"` from the workspace Cargo.toml, so the APK matches the desktop builds. */
val cargoVersion: String = run {
    val manifest = rootDir.parentFile.resolve("Cargo.toml").readText()
    val match = Regex("^version = \"([^\"]+)\"", RegexOption.MULTILINE).find(manifest)
        ?: error("Cargo.toml has no workspace version")
    match.groupValues[1]
}

val cargoVersionCode: Int = run {
    val (major, minor, patch) = cargoVersion.split(".").map { it.takeWhile(Char::isDigit).toInt() }
    major * 10_000 + minor * 100 + patch
}

/** Release signing comes from the environment (CI secrets); without it the APK is unsigned. */
val keystorePath: String? = System.getenv("ANDROID_KEYSTORE_PATH")

android {
    namespace = "io.github.dexsper.cinebox"
    // minSdk is the API level the Rust library is built for (scripts/android-build.sh).
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.dexsper.cinebox"
        minSdk = 26
        targetSdk = 36
        versionName = cargoVersion
        versionCode = cargoVersionCode

        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a")
        }
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            // Nothing to shrink: the app code is Rust, and the verifier is reached only through JNI.
            isMinifyEnabled = false
            if (keystorePath != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }
}

dependencies {
    implementation(libs.androidx.annotation)
    implementation(libs.media3.exoplayer)
    implementation(libs.rustls.platform.verifier)
}
