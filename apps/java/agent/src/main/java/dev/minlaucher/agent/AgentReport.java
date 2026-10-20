package dev.minlaucher.agent;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * The state of a run of the agent.
 *
 * <p>The report is written from several threads — the thread of the
 * game, the thread that writes the report — so everything in here is
 * guarded.
 */
final class AgentReport {

    /** The file the report goes to. */
    private Path output;

    /** When the agent was attached, in nanoseconds. */
    private long startedAt;

    /** The notes of the run. */
    private final List<String> notes = new ArrayList<String>();

    /**
     * Set the file the report goes to.
     *
     * @param output the file
     */
    synchronized void setOutput(Path output) {
        this.output = output;
    }

    /**
     * The file the report goes to.
     *
     * @return the file
     */
    synchronized Path output() {
        return output;
    }

    /**
     * Set the time the agent was attached.
     *
     * @param startedAt the time, in nanoseconds
     */
    synchronized void setStartedAt(long startedAt) {
        this.startedAt = startedAt;
    }

    /**
     * The time the agent was attached.
     *
     * @return the time, in nanoseconds
     */
    synchronized long startedAt() {
        return startedAt;
    }

    /**
     * Add a note to the run.
     *
     * @param note the note
     */
    synchronized void note(String note) {
        notes.add(note);
    }

    /**
     * The notes of the run.
     *
     * @return the notes
     */
    synchronized List<String> notes() {
        return new ArrayList<String>(notes);
    }

    /**
     * The report as a JSON document.
     *
     * <p>The names of the fields are the names the Rust side knows:
     * the launcher reads this document and turns it into the
     * measurements of the metrics M5 and M6.
     *
     * @param counter the counter of the frames
     * @param memoryUsed the heap the game uses, in bytes
     * @return the JSON document
     */
    synchronized String toJson(FrameCounter counter, long memoryUsed) {
        FrameStatistics statistics = counter.statistics();
        StringBuilder out = new StringBuilder(512);

        out.append("{\n");

        // The time the agent was attached, and how long the game has
        // been running.
        out.append("  \"agent_started_at_nanos\": ").append(startedAt).append(",\n");
        out.append("  \"uptime_nanos\": ")
                .append(System.nanoTime() - startedAt)
                .append(",\n");

        // The frames.
        out.append("  \"frames\": ").append(statistics.count()).append(",\n");
        out.append("  \"average_fps\": ")
                .append(Report.number(statistics.averageFps()))
                .append(",\n");
        out.append("  \"average_frametime_ms\": ")
                .append(Report.number(statistics.averageFrametime()))
                .append(",\n");
        out.append("  \"p99_frametime_ms\": ")
                .append(Report.number(statistics.percentileFrametime(99.0)))
                .append(",\n");
        out.append("  \"p95_frametime_ms\": ")
                .append(Report.number(statistics.percentileFrametime(95.0)))
                .append(",\n");
        out.append("  \"p50_frametime_ms\": ")
                .append(Report.number(statistics.percentileFrametime(50.0)))
                .append(",\n");
        out.append("  \"one_percent_low_ms\": ")
                .append(Report.number(statistics.onePercentLow()))
                .append(",\n");

        // The heap of the virtual machine, in mebibytes. It is not the
        // memory of the process: the metric M8 is the working set of
        // the game and is measured from the outside. The heap is a
        // part of the run, and it goes into the metadata of it.
        out.append("  \"heap_used_mib\": ")
                .append(Report.number(memoryUsed / (1024.0 * 1024.0)))
                .append(",\n");

        // Whether the frames were counted at all. When this is false,
        // the numbers above are empty and the report says so.
        out.append("  \"frames_counted\": ")
                .append(statistics.count() > 0)
                .append(",\n");

        // The notes and the failures.
        out.append("  \"notes\": [");
        List<String> allNotes = notes();
        for (int i = 0; i < allNotes.size(); i++) {
            if (i > 0) {
                out.append(", ");
            }
            out.append(Report.quote(allNotes.get(i)));
        }
        out.append("],\n");

        List<String> failures = counter.failures();
        out.append("  \"failures\": [");
        for (int i = 0; i < failures.size(); i++) {
            if (i > 0) {
                out.append(", ");
            }
            out.append(Report.quote(failures.get(i)));
        }
        out.append("]\n");

        out.append("}\n");
        return out.toString();
    }
}
