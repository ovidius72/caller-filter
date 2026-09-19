plugins {
    // AGP 9 bundles Kotlin support; adding kotlin.android as well clashes.
    id("com.android.application")
}

android {
    namespace = "com.antoniopantano.callerfilter"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.antoniopantano.callerfilter"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        // JNA ships additional ABIs; advertise only ones with our Rust core.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Generated UniFFI bindings and the cargo-ndk output are build products,
    // produced by tools/build-android.sh and never committed.
    sourceSets["main"].kotlin.srcDir("${projectDir}/generated/kotlin")
    sourceSets["main"].jniLibs.srcDir("${projectDir}/generated/jniLibs")
    sourceSets["main"].assets.srcDir("${projectDir}/generated/probeAssets")

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

// A missing fixture/native output must not quietly produce a nonfunctional probe.
val verifyProbeInputs = tasks.register("verifyProbeInputs") {
    doLast {
        check(file("generated/probeAssets/probe.properties").isFile) {
            "Prepare a private fixture with tools/device-tests/android-probe.py build"
        }
        check(file("generated/kotlin").walkTopDown().any { it.extension == "kt" }) {
            "Generate fresh bindings with tools/build-android.sh"
        }
        listOf("arm64-v8a", "armeabi-v7a", "x86_64").forEach { abi ->
            check(file("generated/jniLibs/$abi/libcallerfilter_core.so").isFile) {
                "Missing native core for $abi"
            }
        }
    }
}
tasks.named("preBuild") { dependsOn(verifyProbeInputs) }

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    // UniFFI's Kotlin bindings need these at runtime.
    // 5.17 also aligns the x86_64 Android native library to 16 KB.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}
