plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

val releaseSigningValues = mapOf(
    "storeFile" to providers.environmentVariable("DJI_ANDROID_KEYSTORE_PATH").orNull,
    "storePassword" to providers.environmentVariable("DJI_ANDROID_KEYSTORE_PASSWORD").orNull,
    "keyAlias" to providers.environmentVariable("DJI_ANDROID_KEY_ALIAS").orNull,
    "keyPassword" to providers.environmentVariable("DJI_ANDROID_KEY_PASSWORD").orNull,
)
// Facebook login (see FACEBOOK_SETUP.md): the Meta App ID and its Facebook Login for Business
// configuration. Both are public parts of the app; set them in ~/.gradle/gradle.properties or
// the environment.
val facebookAppId = (providers.gradleProperty("facebookAppId").orNull ?: providers.environmentVariable("FACEBOOK_APP_ID").orNull)
    .orEmpty().trim()
val facebookLoginConfigId = (providers.gradleProperty("facebookLoginConfigId").orNull ?: providers.environmentVariable("FACEBOOK_LOGIN_CONFIG_ID").orNull)
    .orEmpty().trim()
if (!facebookAppId.matches(Regex("[0-9]*")) || !facebookLoginConfigId.matches(Regex("[0-9]*"))) {
    throw GradleException("facebookAppId and facebookLoginConfigId must be digits.")
}

val hasAnyReleaseSigningValue = releaseSigningValues.values.any { !it.isNullOrBlank() }
val hasCompleteReleaseSigning = releaseSigningValues.values.all { !it.isNullOrBlank() }

if (hasAnyReleaseSigningValue && !hasCompleteReleaseSigning) {
    throw GradleException(
        "Android release signing is incomplete. Set all DJI_ANDROID_KEYSTORE_* environment variables.",
    )
}

android {
    namespace = "com.streammydrone.app"
    buildToolsVersion = "36.1.0"
    ndkVersion = "28.2.13676358"

    compileSdk {
        version = release(36) {
            minorApiLevel = 1
        }
    }

    defaultConfig {
        applicationId = "com.streammydrone.app"
        minSdk = 28
        targetSdk = 36
        versionCode = 2
        versionName = "1.1"

        ndk {
            abiFilters += "arm64-v8a"
        }

        buildConfigField("String", "FACEBOOK_APP_ID", "\"$facebookAppId\"")
        buildConfigField("String", "FACEBOOK_LOGIN_CONFIG_ID", "\"$facebookLoginConfigId\"")
        // Facebook's login returns to fb<App ID>://authorize; without an App ID nothing may claim "fb".
        resValue("string", "fb_login_protocol_scheme", if (facebookAppId.isEmpty()) "streammydrone-no-facebook" else "fb$facebookAppId")
    }

    signingConfigs {
        if (hasCompleteReleaseSigning) {
            create("release") {
                storeFile = rootProject.file(releaseSigningValues.getValue("storeFile")!!)
                storePassword = releaseSigningValues.getValue("storePassword")
                keyAlias = releaseSigningValues.getValue("keyAlias")
                keyPassword = releaseSigningValues.getValue("keyPassword")
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
                enableV4Signing = true
            }
        }
    }

    buildTypes {
        // "Connect account" for YouTube and Facebook works only for the test users listed with Google
        // and Meta until Google's OAuth verification and Meta's App Review pass, so release builds
        // offer manual stream keys only. Set these to true in release once both reviews are approved.
        debug {
            buildConfigField("boolean", "YOUTUBE_ACCOUNT_CONNECTION", "true")
            buildConfigField("boolean", "FACEBOOK_ACCOUNT_CONNECTION", "true")
        }
        release {
            buildConfigField("boolean", "YOUTUBE_ACCOUNT_CONNECTION", "false")
            buildConfigField("boolean", "FACEBOOK_ACCOUNT_CONNECTION", "false")
            isMinifyEnabled = true
            isShrinkResources = true
            signingConfig = signingConfigs.findByName("release")
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
        resValues = true
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
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.core:core-ktx:1.18.0")
    implementation("com.google.android.gms:play-services-auth:22.0.0")
    // Facebook's login page and broadcast pages open in a Custom Tab (no Facebook SDK).
    implementation("androidx.browser:browser:1.9.0")
    // Turns a heavy test video into what DJI Fly sends (see TestVideoConverter).
    implementation("androidx.media3:media3-common:1.11.1")
    implementation("androidx.media3:media3-effect:1.11.1")
    implementation("androidx.media3:media3-transformer:1.11.1")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")

    debugImplementation("androidx.compose.ui:ui-tooling")
}
