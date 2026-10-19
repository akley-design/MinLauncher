package dev.minlaucher.agent;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Path;
import java.util.Map;
import org.junit.jupiter.api.Test;

/**
 * The tests of the arguments of the agent.
 */
class MinLauncherAgentTest {

    @Test
    void the_arguments_are_read_as_pairs() {
        Map<String, String> options =
                MinLauncherAgent.parseArguments("output=C:\\a\\b.json, interval=10");

        assertEquals("C:\\a\\b.json", options.get("output"));
        assertEquals("10", options.get("interval"));
    }

    @Test
    void an_argument_without_a_value_is_true() {
        Map<String, String> options = MinLauncherAgent.parseArguments("verbose");

        assertEquals("true", options.get("verbose"));
    }

    @Test
    void no_arguments_at_all_are_fine() {
        assertTrue(MinLauncherAgent.parseArguments(null).isEmpty());
        assertTrue(MinLauncherAgent.parseArguments("").isEmpty());
        assertTrue(MinLauncherAgent.parseArguments("  ").isEmpty());
    }

    @Test
    void the_report_goes_where_the_property_says() {
        // The property wins over the option of the agent: the launcher
        // is the one that knows where the report belongs, and the
        // option cannot carry a path that has an '=' in it.
        System.setProperty(MinLauncherAgent.OUTPUT_PROPERTY, "C:\\a\\b.json");
        try {
            Path path = MinLauncherAgent.outputPath(
                    MinLauncherAgent.parseArguments("output=C:\\other.json"));
            assertEquals("C:\\a\\b.json", path.toString());
        } finally {
            System.clearProperty(MinLauncherAgent.OUTPUT_PROPERTY);
        }
    }

    @Test
    void the_option_of_the_agent_is_used_when_there_is_no_property() {
        System.clearProperty(MinLauncherAgent.OUTPUT_PROPERTY);
        Path path = MinLauncherAgent.outputPath(
                MinLauncherAgent.parseArguments("output=C:\\a\\b.json"));
        assertEquals("C:\\a\\b.json", path.toString());
    }

    @Test
    void the_report_goes_to_the_working_directory_when_nothing_is_said() {
        System.clearProperty(MinLauncherAgent.OUTPUT_PROPERTY);
        Path path = MinLauncherAgent.outputPath(MinLauncherAgent.parseArguments(""));

        assertEquals(Report.DEFAULT_FILE_NAME, path.getFileName().toString());
    }

    @Test
    void the_interval_is_read_from_the_property() {
        System.setProperty(MinLauncherAgent.INTERVAL_PROPERTY, "30");
        try {
            assertEquals(30, MinLauncherAgent.intervalSeconds(
                    MinLauncherAgent.parseArguments("interval=5")));
        } finally {
            System.clearProperty(MinLauncherAgent.INTERVAL_PROPERTY);
        }
    }

    @Test
    void a_silly_interval_falls_back_to_the_default() {
        System.clearProperty(MinLauncherAgent.INTERVAL_PROPERTY);

        assertEquals(MinLauncherAgent.DEFAULT_INTERVAL_SECONDS,
                MinLauncherAgent.intervalSeconds(MinLauncherAgent.parseArguments("interval=0")));
        assertEquals(MinLauncherAgent.DEFAULT_INTERVAL_SECONDS,
                MinLauncherAgent.intervalSeconds(MinLauncherAgent.parseArguments("interval=-5")));
        assertEquals(MinLauncherAgent.DEFAULT_INTERVAL_SECONDS,
                MinLauncherAgent.intervalSeconds(MinLauncherAgent.parseArguments("interval=soon")));
        assertEquals(MinLauncherAgent.DEFAULT_INTERVAL_SECONDS,
                MinLauncherAgent.intervalSeconds(MinLauncherAgent.parseArguments("")));
    }
}
