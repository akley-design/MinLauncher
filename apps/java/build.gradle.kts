// The shared configuration of the two modules.
//
// The agent is built for Java 8, because that is the oldest runtime the
// launcher has to support for the versions that predate the modules of
// Minecraft. The mod is built for Java 17, because that is what the
// Fabric loader needs.
//
// Nothing in this build is allowed to fail quietly: a build that breaks
// without saying why is worse than no build at all.
//
// The build directory moves to an ASCII path for Windows projects under
// non-ASCII paths. The JVM test runner can otherwise lose class names
// when it reads the class path through the Windows ANSI code page.

plugins {
    java
}

allprojects {
    group = "dev.minlaucher"
    version = property("modVersion") as String

    // Keep Windows builds outside a non-ASCII path, where the JVM test
    // runner can lose class names. Other platforms use standard paths,
    // which are also where CI checks for the jars.
    val configuredBuildDir = findProperty("org.gradle.project.buildDir") as String?
    val needsAsciiPath = System.getProperty("os.name").startsWith("Windows") &&
        rootProject.projectDir.path.any { it.code > 127 }
    val relocated = configuredBuildDir ?: if (needsAsciiPath) {
        "C:/Users/Public/minlaucher-java-build"
    } else {
        null
    }
    if (relocated != null) {
        val suffix = if (this == rootProject) "" else "/${name}"
        layout.buildDirectory.set(file(relocated + suffix))
    }
}

subprojects {
    tasks.withType<JavaCompile>().configureEach {
        options.encoding = "UTF-8"
        options.compilerArgs.add("-Xlint:all")
        // The parameters of the deprecated methods of the JDK are not
        // worth a warning in a project that has to run on Java 8.
        options.compilerArgs.add("-Xlint:-removal")
    }

    tasks.withType<Test>().configureEach {
        useJUnitPlatform()
        testLogging {
            events("passed", "skipped", "failed")
        }
    }
}
