plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
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

    buildFeatures {
        compose = true
    }

    sourceSets["main"].java.srcDir(layout.buildDirectory.dir("generated/uniffi/kotlin"))
    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")

    testOptions {
        unitTests.all {
            it.systemProperty(
                "jna.library.path",
                "${rootProject.projectDir}/../target/debug;${rootProject.projectDir}/app/src/main/jniLibs/x86_64"
            )
        }
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.activity:activity-compose:1.12.3")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    testImplementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("org.json:json:20240303")

    androidTestImplementation(platform("androidx.compose:compose-bom:2026.09.00"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    debugAndroidTestImplementation("androidx.compose.ui:ui-test-manifest")
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
