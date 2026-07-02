// SPDX-License-Identifier: GPL-3.0-or-later

package com.ichi2.anki.gradle

import org.gradle.api.Project
import org.gradle.kotlin.dsl.DependencyHandlerScope
import java.io.File
import java.util.Properties

// The Anki backend (rsdroid) is consumed as the in-tree, locally-built artifact in
// android/backend by default (this is a monorepo built against the in-tree core).
// The published library from the version catalog is pinned to an older Anki release
// and is only used if explicitly opted in with `local_backend=false` in local.properties.

// Paths within the in-tree `android/backend` build.
private const val RSDROID_AAR =
    "rsdroid/build/outputs/aar/rsdroid-release.aar"
private const val RSDROID_TESTING_JAR =
    "rsdroid-testing/build/libs/rsdroid-testing.jar"

/**
 * Adds the Anki backend (rsdroid) dependencies, choosing between the locally-built
 * artifacts and the published version-catalog libraries based on `local_backend` in
 * `local.properties`.
 *
 * See https://github.com/ankidroid/Anki-Android-Backend/
 */
// TODO: use context parameters when we compile kts with Kotlin 2.4.0
fun DependencyHandlerScope.addAnkiBackendDependencies(project: Project) {
    with(project) {
        val useLocalBackend = localBackendEnabled()

        addBackendArtifact("implementation", useLocalBackend, RSDROID_AAR, "ankiBackend-backend")
        addBackendArtifact("testImplementation", useLocalBackend, RSDROID_TESTING_JAR, "ankiBackend-testing")

        // protobuf is required when loading from a file, regardless of the backend source
        dependencies.addProvider("implementation", libsLibrary("protobuf-kotlin-lite"))

        if (project.hasTestFixtures) {
            addBackendArtifact("testFixturesImplementation", useLocalBackend, RSDROID_AAR, "ankiBackend-backend")
            addBackendArtifact("testFixturesImplementation", useLocalBackend, RSDROID_TESTING_JAR, "ankiBackend-testing")
            dependencies.addProvider("testFixturesImplementation", libsLibrary("protobuf-kotlin-lite"))
        }
    }
}

private val Project.hasTestFixtures: Boolean
    get() = configurations.findByName("testFixturesImplementation") != null

private fun Project.localBackendEnabled(): Boolean {
    // Default to the in-tree backend; only fall back to the published library when a
    // developer explicitly sets `local_backend=false` in local.properties.
    val localProperties = rootProject.layout.projectDirectory.file("local.properties")
    val content = providers.fileContents(localProperties).asText.orNull ?: return true
    val properties = Properties().apply { content.reader().use { load(it) } }
    return properties["local_backend"] != "false"
}

private fun Project.addBackendArtifact(
    configuration: String,
    useLocalBackend: Boolean,
    localPath: String,
    catalogAlias: String,
) {
    if (useLocalBackend) {
        // Monorepo layout: app is android/app, backend is android/backend
        // (../backend relative to the app's rootProject dir).
        val backendCheckout = File(rootProject.projectDir.parentFile, "backend")
        dependencies.add(configuration, files(File(backendCheckout, localPath)))
    } else {
        dependencies.addProvider(configuration, libsLibrary(catalogAlias))
    }
}
