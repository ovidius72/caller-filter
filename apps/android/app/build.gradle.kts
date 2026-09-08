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
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Generated UniFFI bindings and the cargo-ndk output are build products,
    // produced by tools/build-android.sh and never committed.
    sourceSets["main"].kotlin.srcDir("${projectDir}/generated/kotlin")
    sourceSets["main"].jniLibs.srcDir("${projectDir}/generated/jniLibs")

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    // UniFFI's Kotlin bindings need these at runtime.
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}
