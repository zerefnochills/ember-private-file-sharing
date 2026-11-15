import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.ember.mobile"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.ember.mobile"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.0.1"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = false
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

    buildFeatures {
        compose = true
    }

    sourceSets["main"].jniLibs.directories.add(file("src/main/jniLibs").absolutePath)
    sourceSets["main"].assets.directories.add(rootProject.projectDir.parentFile.resolve("testvectors").absolutePath)

    testOptions {
        unitTests.all {
            it.systemProperty(
                "jna.library.path",
                "${rootProject.projectDir}/../target/debug;${rootProject.projectDir}/app/src/main/jniLibs/x86_64"
            )
        }
    }
}

// AGP 9's built-in Kotlin integration does not discover extra Kotlin files from
// Android's Java source directory collection, so add UniFFI output to app variants.
tasks.withType<KotlinCompile>().matching {
    it.name.startsWith("compile") && it.name.endsWith("Kotlin") &&
        !it.name.contains("UnitTest") && !it.name.contains("AndroidTest")
}.configureEach {
    source(layout.buildDirectory.dir("generated/uniffi/kotlin"))
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.activity:activity-compose:1.12.3")
    implementation("androidx.camera:camera-camera2:1.6.2")
    implementation("androidx.camera:camera-lifecycle:1.6.2")
    implementation("androidx.camera:camera-view:1.6.2")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("com.google.zxing:core:3.5.4")
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    testImplementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("org.json:json:20240303")

    androidTestImplementation(platform("androidx.compose:compose-bom:2026.09.00"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

tasks.register("checkForbiddenDependencies") {
    group = "verification"
    description = "Fails if any forbidden Google dependencies appear in the project."
    doLast {
        val forbidden = listOf(
            "com.google.android.gms",
            "com.google.firebase",
            "com.google.mlkit",
            "play-services",
            "firebase",
            "mlkit",
            "nearby"
        )
        val violations = mutableListOf<String>()
        configurations.matching { it.isCanBeResolved }.forEach { config ->
            try {
                config.resolvedConfiguration.resolvedArtifacts.forEach { artifact ->
                    val id = artifact.moduleVersion.id.toString().lowercase()
                    for (banned in forbidden) {
                        if (id.contains(banned)) {
                            violations.add("Forbidden dependency found in ${config.name}: $id (matches banned term '$banned')")
                        }
                    }
                }
            } catch (_: Exception) {
                // Ignore configurations that cannot be resolved in this context
            }
        }
        if (violations.isNotEmpty()) {
            throw GradleException("Android dependency check failed with forbidden Google dependencies:\n" + violations.joinToString("\n"))
        }
        println("Dependency check passed: zero forbidden Google dependencies found.")
    }
}
