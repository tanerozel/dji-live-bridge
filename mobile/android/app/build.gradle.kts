plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.djilivebridge.android"

    compileSdk {
        version = release(36) {
            minorApiLevel = 1
        }
    }

    defaultConfig {
        applicationId = "com.djilivebridge.android"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets["main"].jniLibs.directories.add("build/generated/rustJniLibs")

    packaging {
        jniLibs {
            // librtmp2 is statically linked into our JNI library; cargo-ndk also emits
            // the crate's standalone cdylib, which is not loaded by the app.
            excludes += "**/liblibrtmp2-*.so"
        }
    }
}

val buildRustRelay = tasks.register<Exec>("buildRustRelay") {
    group = "build"
    description = "Builds the arm64 Android RTMP relay core with cargo-ndk."
    workingDir(rootProject.projectDir)
    commandLine(
        "sh",
        rootProject.file("scripts/build-rust.sh").absolutePath,
        layout.buildDirectory.dir("generated/rustJniLibs").get().asFile.absolutePath,
    )
    inputs.files(
        rootProject.file("scripts/build-rust.sh"),
        rootProject.file("../relay-core/Cargo.toml"),
        rootProject.file("../relay-core/Cargo.lock"),
        rootProject.fileTree("../relay-core/src"),
        rootProject.fileTree("../relay-core/vendor/librtmp2/src"),
    )
    outputs.dir(layout.buildDirectory.dir("generated/rustJniLibs"))
}

tasks.named("preBuild").configure {
    dependsOn(buildRustRelay)
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.06.00")

    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.core:core-ktx:1.18.0")

    debugImplementation("androidx.compose.ui:ui-tooling")
}
