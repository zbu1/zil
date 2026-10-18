plugins {
    id("com.android.application") version "9.4.1"
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21"
}

val workspace = rootDir.parentFile

android {
    namespace = "io.github.zbu1.zil"
    compileSdk = 37
    defaultConfig {
        applicationId = "io.github.zbu1.zil"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = Regex("""version = "(.+)"""").find(workspace.resolve("Cargo.toml").readText())!!.groupValues[1]
        ndk { abiFilters += "arm64-v8a" }
    }
    buildFeatures { compose = true }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.13.0")
}

// The REPL itself: crates/zil-android, built for the phone by cargo-ndk into jniLibs.
val cargo = tasks.register<Exec>("cargo") {
    workingDir = workspace
    environment("ANDROID_HOME", androidComponents.sdkComponents.sdkDirectory.get().asFile.path)
    commandLine("cargo", "ndk", "-t", "arm64-v8a", "-P", "26", "-o", file("src/main/jniLibs").path, "build", "--release", "-p", "zil-android")
}
tasks.named("preBuild") { dependsOn(cargo) }
