// The profiling agent.
//
// The agent is attached to the game with `-javaagent` and it has two
// jobs:
//
// * it measures how long the game took to start — the part of the
//   metric M3 that happens after the process exists;
//* it measures the frame times of the game, which give the metrics
//   M5 and M6.
//
// The agent has to work on the *official* jar of the game, the one
// that Mojang obfuscates. So it cannot name a class of the game: the
// name of `net.minecraft.client.Minecraft` in the jar is something
// like `fij`, and it differs from version to version.
//
// What Mojang does not obfuscate is the libraries the game uses. The
// frame is put on the screen by a call to a native method of LWJGL,
// and that call is the same in every version of the game:
//
// * `org.lwjgl.glfw.GLFW.glfwSwapBuffers(long)` — LWJGL 3, the game of
//   1.17 and later;
// * `org.lwjgl.opengl.Display.update()` — LWJGL 2, the game up to
//   1.12.2.
//
// The agent weaves a body around those calls, and counts the frames.

plugins {
    java
}

dependencies {
    // ASM weaves the body around the frame swap. The version is
    // pinned to the one that reads the class files of the newest
    // runtimes: the game of this project runs on Java 25.
    implementation("org.ow2.asm:asm:9.9")
    implementation("org.ow2.asm:asm-commons:9.9")

    testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

// The agent is loaded by the JVM itself, before the game's classpath
// exists, and the class loader of the agent is the loader of the
// system — it does not see the class path of the launcher.
//
// That means the jar has to carry everything it needs inside it: a
// plain jar fails with "NoClassDefFoundError: org/objectweb/asm/
// ClassVisitor" the moment it meets the first class to weave.
//
// The jar of the agent is therefore a fat one: the classes of ASM are
// unpacked into it, and the manifest says that they are the ones to
// load.
val agentJar by tasks.registering(Jar::class) {
    archiveBaseName.set("minlaucher-agent")
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE

    from(sourceSets.main.get().output)
    from({
        configurations.runtimeClasspath.get()
            .filter { it.name.endsWith(".jar") }
            .map { zipTree(it) }
    })

    manifest {
        attributes(
            "Premain-Class" to "dev.minlaucher.agent.MinLauncherAgent",
            "Agent-Class" to "dev.minlaucher.agent.MinLauncherAgent",
            "Can-Redefine-Classes" to "true",
            "Can-Retransform-Classes" to "true",
            "Main-Class" to "dev.minlaucher.agent.MinLauncherAgent",
        )
    }

    // The signatures of the dependencies do not survive the unpacking,
    // and the jar is not signed anyway.
    exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA", "META-INF/maven/**")
}

// The plain jar is not the one that is shipped: it cannot weave
// anything, because it does not carry ASM.
tasks.jar {
    enabled = false
    archiveClassifier.set("thin")
}

tasks.assemble {
    dependsOn(agentJar)
}

// The agent is loaded into the game, and the game runs on the runtime
// of the player. The oldest runtime that has to work is Java 8, so
// the agent is compiled for it.
//
// `release` is used rather than `sourceCompatibility` and
// `targetCompatibility`: it checks the API against the *runtime* as
// well, so a method that does not exist in Java 8 is an error and not
// a surprise at run time.
tasks.withType<JavaCompile>().configureEach {
    options.release.set(8)
    // Java 8 is old enough that javac warns about it on every build.
    options.compilerArgs.add("-Xlint:-options")
}
