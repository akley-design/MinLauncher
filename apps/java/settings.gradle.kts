// The Java part of MinLauncher.
//
// Two modules, with two different jobs:
//
// * `agent` — a Java agent that is attached to the game with `-javaagent`.
//   It measures how long the game took to start and how the game behaves
//   while it runs. It works on the official jar of the game, so it needs
//   no loader and no mapping: it hooks the libraries that Mojang does not
//   obfuscate.
//
// * `mod` — a Fabric mod that changes how the game renders and behaves.
//   This is where the optimisation of the game itself lives.

pluginManagement {
    repositories {
        maven("https://maven.fabricmc.net/") {
            name = "Fabric"
        }
        gradlePluginPortal()
        mavenCentral()
    }
}

dependencyResolutionManagement {
    repositories {
        maven("https://maven.fabricmc.net/") {
            name = "Fabric"
        }
        mavenCentral()
    }
}

rootProject.name = "minlaucher-java"

include("agent")
include("mod")
