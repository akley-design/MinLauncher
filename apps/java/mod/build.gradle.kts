// The optimisation mod.
//
// The agent watches the game. The mod *changes* it: this is where the
// promise that the game itself runs faster is kept.
//
// The mod is a Fabric mod, because Fabric is the loader that can be
// used on the official jar of the game without a developer account
// and without a launcher of its own. The launcher installs the loader
// and the mod next to the game, and the game then starts with them.
//
// The mod is built against a version of the game that the toolchain
// supports, and not against the newest one: a mod that does not build
// is worse than no mod at all. The launcher decides at run time which
// mod jar goes with which version of the game.

import org.gradle.api.tasks.Copy

plugins {
    id("fabric-loom") version "1.9-SNAPSHOT"
    id("maven-publish")
}

val modVersion: String by project
val minecraftVersion: String by project
val loaderVersion: String by project

val yarnMappings: String = "1.21.4+build.8"

dependencies {
    minecraft("com.mojang:minecraft:$minecraftVersion")
    mappings("net.fabricmc:yarn:$yarnMappings:v2")
    modImplementation("net.fabricmc:fabric-loader:$loaderVersion")

    testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

// The mod is a library of its own, and the launcher puts it on the
// class path of the game.
loom {
    runs {
        named("client") {
            client()
            ideConfigGenerated(true)
        }
    }
}

tasks.processResources {
    val properties = mapOf(
        "version" to modVersion,
        "minecraft_version" to minecraftVersion,
        "loader_version" to loaderVersion,
        "java_version" to "21",
    )
    inputs.properties(properties)

    filesMatching("fabric.mod.json") {
        expand(properties)
    }
}

java {
    withSourcesJar()
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.withType<JavaCompile>().configureEach {
    options.encoding = "UTF-8"
    options.release.set(21)
}

// The jar of the mod is copied into a directory of its own, with the
// name the launcher looks for. The launcher does not read the name of
// the jar of the mod — it is the one that the settings of the version
// point at — but a stable name makes the installation readable.
val stageMod = tasks.register<Copy>("stageMod") {
    from(tasks.remapJar)
    into(layout.buildDirectory.dir("staged"))
    rename { "minlaucher-optimisations-$modVersion.jar" }
}

tasks.assemble {
    dependsOn(stageMod)
}
