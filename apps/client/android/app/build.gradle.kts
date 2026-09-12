plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

dependencies {
    implementation("androidx.health.connect:connect-client:1.1.0")
}

val rustBuildScript = layout.projectDirectory.file("../build_rust.sh")
val rustAbis = providers.gradleProperty("target-platform").map { platforms ->
    platforms.split(',').map { platform ->
        when (platform.trim()) {
            "android-arm64" -> "arm64-v8a"
            "android-x64" -> "x86_64"
            else -> throw GradleException("Unsupported Flutter target platform: ${platform.trim()}")
        }
    }.distinct().joinToString(" ")
}.orElse("arm64-v8a")
val requestedRustMode = when {
    gradle.startParameter.taskNames.any { it.contains("release", ignoreCase = true) } -> "release"
    gradle.startParameter.taskNames.any { it.contains("profile", ignoreCase = true) } -> "profile"
    else -> "debug"
}
val rustConfiguration = providers.gradleProperty("flutter.buildMode").orElse(requestedRustMode).map { mode ->
    when (mode.lowercase()) {
        "debug" -> "Debug"
        "profile" -> "Profile"
        "release" -> "Release"
        else -> throw GradleException("Unsupported Flutter build mode: $mode")
    }
}
val generatedRustJniLibs = layout.buildDirectory.dir("generated/rust/jniLibs").get().asFile
val buildRust = tasks.register<Exec>("buildRust") {
    environment("FLOE_ANDROID_JNI_LIBS_DIR", generatedRustJniLibs.absolutePath)
    environment("FLOE_ANDROID_ABIS", rustAbis.get())
    environment("CONFIGURATION", rustConfiguration.get())
    commandLine(rustBuildScript.asFile.absolutePath)
}

tasks.named("preBuild") {
    dependsOn(buildRust)
}

android {
    namespace = "app.floe.floe_client"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets.getByName("main").jniLibs.srcDir(generatedRustJniLibs)

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "app.floe.floe_client"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = 26
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName

        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
