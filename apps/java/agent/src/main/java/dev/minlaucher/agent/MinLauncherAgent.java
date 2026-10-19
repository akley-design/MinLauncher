package dev.minlaucher.agent;

import java.io.IOException;
import java.lang.instrument.Instrumentation;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryMXBean;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * The profiling agent of the game.
 *
 * <p>The agent is attached to the game with {@code -javaagent} and it
 * measures two things:
 *
 * <ul>
 *   <li>how long the game took to start — the part of the metric M3
 *       that happens after the process of the game exists;</li>
 *   <li>how the game behaves while it runs — the metrics M5 and M6.</li>
 * </ul>
 *
 * <p>The agent is told where to write its report with an argument:
 *
 * <pre>{@code -javaagent:agent.jar=output=C:\path\to\report.json}</pre>
 *
 * <p>The report is written every few seconds while the game runs, and
 * once more when the game ends. The launcher reads it after the game
 * is over.
 */
public final class MinLauncherAgent {

    /** The name of the argument that says where the report goes. */
    static final String OUTPUT_ARGUMENT = "output";

    /** The name of the argument that says how often the report is written. */
    static final String INTERVAL_ARGUMENT = "interval";

    /** How often the report is written, in seconds. */
    static final int DEFAULT_INTERVAL_SECONDS = 5;

    /** The name of the thread that writes the report. */
    private static final String REPORTER = "minlauncher-reporter";

    /** The report of this run. */
    private static final AgentReport REPORT = new AgentReport();

    private MinLauncherAgent() {
    }

    /**
     * Attach the agent to the game.
     *
     * <p>This is the entry point the JVM calls when the agent is
     * attached at the start of the process. Everything that goes wrong
     * here is swallowed on purpose: an agent that fails must not stop
     * the game from starting.
     *
     * @param arguments the arguments of the agent
     * @param instrumentation the handle of the instrumentation
     */
    public static void premain(String arguments, Instrumentation instrumentation) {
        start(arguments, instrumentation);
    }

    /**
     * Attach the agent to a process that is already running.
     *
     * @param arguments the arguments of the agent
     * @param instrumentation the handle of the instrumentation
     */
    public static void agentmain(String arguments, Instrumentation instrumentation) {
        start(arguments, instrumentation);
    }

    /**
     * Attach the agent.
     *
     * @param arguments the arguments of the agent
     * @param instrumentation the handle of the instrumentation
     */
    private static void start(String arguments, Instrumentation instrumentation) {
        try {
            Map<String, String> options = parseArguments(arguments);
            Path output = outputPath(options);
            REPORT.setOutput(output);
            REPORT.setStartedAt(System.nanoTime());

            // The counter of the frames is put in place before the
            // transformer is added, so that the woven code never
            // finds it missing.
            FrameCounter counter = FrameCounter.get();
            instrumentation.addTransformer(new FrameHook(counter), true);

            startReporter(options);

            REPORT.note("the agent is attached");
        } catch (Throwable failure) {
            // The game must start even when the agent does not.
            System.err.println("[minlauncher] the agent failed: " + failure);
        }
    }

    /**
     * Read the arguments of the agent.
     *
     * <p>The format is the one of the JVM: a list of `key=value`
     * pairs, separated by commas.
     *
     * @param arguments the arguments
     * @return the options
     */
    static Map<String, String> parseArguments(String arguments) {
        Map<String, String> options = new LinkedHashMap<String, String>();
        if (arguments == null || arguments.trim().isEmpty()) {
            return options;
        }

        for (String pair : arguments.split(",")) {
            String trimmed = pair.trim();
            if (trimmed.isEmpty()) {
                continue;
            }
            int equals = trimmed.indexOf('=');
            if (equals < 0) {
                options.put(trimmed, "true");
            } else {
                options.put(
                        trimmed.substring(0, equals).trim(),
                        trimmed.substring(equals + 1).trim());
            }
        }
        return options;
    }

    /**
     * The file the report goes to.
     *
     * @param options the options of the agent
     * @return the file
     */
    static Path outputPath(Map<String, String> options) {
        String output = options.get(OUTPUT_ARGUMENT);
        if (output == null || output.trim().isEmpty()) {
            // Without a file to write to, the report goes to the
            // working directory of the game, under a name the launcher
            // knows.
            return Paths.get(Report.DEFAULT_FILE_NAME).toAbsolutePath();
        }
        return Paths.get(output.trim()).toAbsolutePath();
    }

    /**
     * How often the report is written, in seconds.
     *
     * @param options the options of the agent
     * @return the interval
     */
    static int intervalSeconds(Map<String, String> options) {
        String value = options.get(INTERVAL_ARGUMENT);
        if (value == null) {
            return DEFAULT_INTERVAL_SECONDS;
        }
        try {
            int seconds = Integer.parseInt(value.trim());
            return seconds > 0 ? seconds : DEFAULT_INTERVAL_SECONDS;
        } catch (NumberFormatException ignored) {
            return DEFAULT_INTERVAL_SECONDS;
        }
    }

    /**
     * Start the thread that writes the report.
     *
     * @param options the options of the agent
     */
    private static void startReporter(Map<String, String> options) {
        int interval = intervalSeconds(options);

        Thread reporter = new Thread(new Runnable() {
            @Override
            public void run() {
                while (!Thread.currentThread().isInterrupted()) {
                    try {
                        Thread.sleep(interval * 1000L);
                    } catch (InterruptedException interrupted) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                    writeReport();
                }
            }
        }, REPORTER);

        // The thread must not keep the game alive on its own.
        reporter.setDaemon(true);
        reporter.start();

        // A hook that runs when the game ends, so that the last report
        // is written even if the game was killed.
        Runtime.getRuntime().addShutdownHook(new Thread(new Runnable() {
            @Override
            public void run() {
                writeReport();
            }
        }, "minlauncher-shutdown"));
    }

    /**
     * Write the report.
     */
    static void writeReport() {
        try {
            Report.write(REPORT.output(), REPORT.toJson(FrameCounter.get(), memoryUsed()));
        } catch (IOException failure) {
            System.err.println("[minlauncher] cannot write the report: " + failure);
        }
    }

    /**
     * The memory the game uses, in bytes.
     *
     * @return the used heap, or zero when it cannot be read
     */
    static long memoryUsed() {
        try {
            MemoryMXBean memory = ManagementFactory.getMemoryMXBean();
            return memory.getHeapMemoryUsage().getUsed();
        } catch (Throwable ignored) {
            return 0L;
        }
    }
}
