import org.jetbrains.kotlin.gradle.tasks.KotlinCompile
import java.net.HttpURLConnection
import java.net.URI
import java.util.concurrent.TimeUnit

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
        val configuredEmberLinkHost = providers.gradleProperty("emberLinkHost")
            .orElse("links.ember.invalid")
            .get()
        require(configuredEmberLinkHost.matches(Regex("[A-Za-z0-9.-]+"))) {
            "emberLinkHost must be a hostname without a scheme, port, or path"
        }
        manifestPlaceholders["emberLinkHost"] = configuredEmberLinkHost
        buildConfigField("String", "EMBER_LINK_HOST", "\"$configuredEmberLinkHost\"")
        val configuredRelayHost = providers.gradleProperty("emberRelayHost")
            .orElse(configuredEmberLinkHost)
            .get()
        require(configuredRelayHost.matches(Regex("[A-Za-z0-9.-]+"))) {
            "emberRelayHost must be a hostname without a scheme, port, or path"
        }
        buildConfigField("String", "EMBER_RELAY_HOST", "\"$configuredRelayHost\"")
        val grievanceEmail = providers.gradleProperty("emberGrievanceEmail")
            .orElse("grievance@example.invalid")
            .get()
        require(grievanceEmail.matches(Regex("[A-Za-z0-9._+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}"))) {
            "emberGrievanceEmail must be an email address"
        }
        buildConfigField("String", "EMBER_GRIEVANCE_EMAIL", "\"$grievanceEmail\"")
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
        buildConfig = true
    }

    sourceSets["main"].jniLibs.directories.add(file("src/main/jniLibs").absolutePath)
    sourceSets["main"].assets.directories.add(rootProject.projectDir.parentFile.resolve("testvectors").absolutePath)
    sourceSets["main"].assets.directories.add(rootProject.projectDir.parentFile.resolve("docs/legal").absolutePath)

    testOptions {
        unitTests.all {
            it.systemProperty(
                "jna.library.path",
                "${rootProject.projectDir}/../target/debug;${rootProject.projectDir}/app/src/main/jniLibs/x86_64"
            )
        }
    }
}

val hostWorkspace = rootProject.projectDir.parentFile
val testRelayServerKey = "emberAndroidTestRelayProcess"

val startEmberAndroidTestRelay = tasks.register("startEmberAndroidTestRelay") {
    group = "verification"
    description = "Builds and starts the real ember-server router for emulator integration tests."
    doLast {
        try {
            val existing = URI.create("http://127.0.0.1:8080/v1/pow/challenge")
                .toURL().openConnection() as HttpURLConnection
            existing.connectTimeout = 500
            existing.readTimeout = 500
            val alreadyRunning = existing.responseCode == 200
            existing.disconnect()
            if (alreadyRunning) {
                project.extensions.extraProperties[testRelayServerKey] = null
                logger.lifecycle("Reusing the existing ember-server test relay on host port 8080.")
                return@doLast
            }
        } catch (_: Exception) {
            // Start the managed test relay below when no compatible relay is listening.
        }

        val manifest = hostWorkspace.resolve("android/test-relay-server/Cargo.toml")
        val cargoBuild = ProcessBuilder(
            "cargo", "build", "--manifest-path", manifest.absolutePath,
            "--bin", "ember-android-test-server",
        ).directory(hostWorkspace).inheritIO().apply {
            environment()["CARGO_TARGET_DIR"] = hostWorkspace.resolve("target").absolutePath
        }.start()
        check(cargoBuild.waitFor() == 0) { "Could not build the Android real-server test harness" }

        val isWindows = System.getProperty("os.name").lowercase().contains("windows")
        val executable = hostWorkspace.resolve(
            "target/debug/ember-android-test-server" + if (isWindows) ".exe" else "",
        )
        check(executable.isFile) { "Built Android real-server test harness is missing: $executable" }
        val log = layout.buildDirectory.file("test-relay-server.log").get().asFile
        log.parentFile.mkdirs()
        val process = ProcessBuilder(executable.absolutePath)
            .directory(hostWorkspace)
            .redirectErrorStream(true)
            .redirectOutput(log)
            .start()
        project.extensions.extraProperties[testRelayServerKey] = process

        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(20)
        var ready = false
        while (System.nanoTime() < deadline && process.isAlive && !ready) {
            try {
                val connection = URI.create("http://127.0.0.1:8080/v1/pow/challenge").toURL()
                    .openConnection() as HttpURLConnection
                connection.connectTimeout = 500
                connection.readTimeout = 500
                ready = connection.responseCode == 200
                connection.disconnect()
            } catch (_: Exception) {
                Thread.sleep(200)
            }
        }
        if (!ready) {
            process.destroyForcibly()
            process.waitFor(5, TimeUnit.SECONDS)
            error("The real ember-server test harness did not start on port 8080. Log: ${log.readText()}")
        }
        logger.lifecycle("Started ember-server router for emulator tests (host port 8080).")
    }
}

val stopEmberAndroidTestRelay = tasks.register("stopEmberAndroidTestRelay") {
    group = "verification"
    description = "Stops the real ember-server test process after emulator instrumentation."
    doLast {
        val process = project.extensions.extraProperties.properties[testRelayServerKey] as? Process
        if (process != null) {
            if (process.isAlive) {
                val isWindows = System.getProperty("os.name").lowercase().contains("windows")
                val killCommand = if (isWindows) {
                    listOf("taskkill.exe", "/PID", process.pid().toString(), "/T", "/F")
                } else {
                    listOf("kill", "-TERM", process.pid().toString())
                }
                val killer = ProcessBuilder(killCommand).start()
                check(killer.waitFor() == 0) { "Could not signal Android real-server test process ${process.pid()}" }
                check(process.waitFor(5, TimeUnit.SECONDS)) {
                    "Could not stop Android real-server test process ${process.pid()}"
                }
            }
            project.extensions.extraProperties[testRelayServerKey] = null
            logger.lifecycle("Stopped ember-server test process.")
        }
    }
}

tasks.matching { it.name == "connectedDebugAndroidTest" }.configureEach {
    dependsOn(startEmberAndroidTestRelay)
    finalizedBy(stopEmberAndroidTestRelay)
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
    implementation("androidx.appcompat:appcompat:1.7.1")
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
            "nearby",
            "analytics",
            "crashlytics",
            "sentry",
            "bugsnag",
            "appcenter",
            "hockeyapp",
            "rollbar",
            "mixpanel",
            "amplitude",
            "segment",
            "adjust",
            "appsflyer",
            "admob",
            "adservices",
            "applovin",
            "unity-ads",
            "ironsource",
            "chartboost",
            "adcolony",
            "startapp",
        )
        val violations = mutableListOf<String>()
        // Lint's own implementation embeds an analytics-library tracker. Build tools are
        // not shipped in the APK, so scan app/test dependency graphs and exclude lint tools.
        configurations.filter {
            it.isCanBeResolved && !it.name.contains("lint", ignoreCase = true)
        }.toList().forEach { config ->
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
