pluginManagement {
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
        // The Kotlin half of rustls-platform-verifier ships inside the Rust crate, not on Maven.
        maven {
            url = uri(rustlsPlatformVerifierMaven())
            metadataSources.artifact()
        }
    }
}

rootProject.name = "cinebox"
include(":app")

fun rustlsPlatformVerifierMaven(): File {
    val metadata = providers.exec {
        workingDir = rootDir.parentFile
        commandLine(
            "cargo", "metadata", "--format-version", "1",
            "--filter-platform", "aarch64-linux-android",
            "--manifest-path", "crates/cinebox-android/Cargo.toml",
        )
    }.standardOutput.asText.get()

    @Suppress("UNCHECKED_CAST")
    val packages = (groovy.json.JsonSlurper().parseText(metadata) as Map<String, Any>)["packages"]
        as List<Map<String, Any>>
    val manifest = packages.first { it["name"] == "rustls-platform-verifier-android" }["manifest_path"]
        as String

    return File(File(manifest).parentFile, "maven")
}
