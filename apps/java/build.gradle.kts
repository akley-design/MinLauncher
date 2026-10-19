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
// The build directory is relocated when `org.gradle.project.buildDir`
// is set, which is done in `gradle.properties`. The reason is in that
// file: the JVM reads the class path of the test runner through the
// Windows ANSI code page, and a path with non-ASCII characters in it
// makes the test runner lose every class it was given.

plugins {
    java
}

allprojects {
    group = "dev.minlaucher"
    version = property("modVersion") as String

    // The build directory of the project, when it was moved. Every
    // project keeps its own directory under the moved root, so that the
    // two modules do not write into the same place.
    val relocated = findProperty("org.gradle.project.buildDir") as String?
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
