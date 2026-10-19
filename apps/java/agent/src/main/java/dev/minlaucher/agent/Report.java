package dev.minlaucher.agent;

import java.io.IOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

/**
 * Writes the report of a run.
 *
 * <p>The report is a JSON document, and the launcher reads it after the
 * game ends. The format is deliberately dull: a flat object with a
 * handful of numbers, so that the Rust side can read it with a
 * deserialiser that has nothing to decide.
 *
 * <p>The report is written in two steps. While the game runs, the
 * agent writes the numbers it already has every few seconds, so that a
 * report exists even if the game is killed. When the game ends, the
 * final report is written over it.
 *
 * <p>Writing is atomic where it can be: the report is written to a
 * file next to the target and then moved over it, so the launcher
 * never reads half a document.
 */
final class Report {

    /** The name of the file the launcher looks for when it knows nothing else. */
    static final String DEFAULT_FILE_NAME = "minlauncher-agent.json";

    private Report() {
    }

    /**
     * Write a report.
     *
     * @param target the file to write
     * @param content the JSON document
     * @throws IOException when the file cannot be written
     */
    static void write(Path target, String content) throws IOException {
        Path parent = target.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }

        // The report is written next to the target and then moved over
        // it, so that a reader never sees half a document.
        Path temporary = target.resolveSibling(target.getFileName() + ".tmp");
        try (Writer writer = Files.newBufferedWriter(
                temporary,
                StandardCharsets.UTF_8,
                StandardOpenOption.CREATE,
                StandardOpenOption.TRUNCATE_EXISTING,
                StandardOpenOption.WRITE)) {
            writer.write(content);
        }

        try {
            Files.move(
                    temporary,
                    target,
                    java.nio.file.StandardCopyOption.REPLACE_EXISTING,
                    java.nio.file.StandardCopyOption.ATOMIC_MOVE);
        } catch (java.nio.file.AtomicMoveNotSupportedException ignored) {
            // Some file systems cannot move a file atomically. The
            // document is whole either way, so a plain move is enough.
            Files.move(temporary, target, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        }
    }

    /**
     * Escape a string for the JSON document.
     *
     * @param value the string
     * @return the string, quoted and escaped
     */
    static String quote(String value) {
        if (value == null) {
            return "null";
        }
        StringBuilder out = new StringBuilder(value.length() + 2);
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char character = value.charAt(i);
            switch (character) {
                case '"':
                    out.append("\\\"");
                    break;
                case '\\':
                    out.append("\\\\");
                    break;
                case '\n':
                    out.append("\\n");
                    break;
                case '\r':
                    out.append("\\r");
                    break;
                case '\t':
                    out.append("\\t");
                    break;
                case '\b':
                    out.append("\\b");
                    break;
                case '\f':
                    out.append("\\f");
                    break;
                default:
                    if (character < 0x20) {
                        out.append(String.format("\\u%04x", (int) character));
                    } else {
                        out.append(character);
                    }
                    break;
            }
        }
        out.append('"');
        return out.toString();
    }

    /**
     * Format a number for the JSON document.
     *
     * <p>A number that is not finite cannot be written as JSON, and it
     * would mean that something went wrong with the measurement. It is
     * written as zero, and the launcher sees a number rather than a
     * broken document.
     *
     * @param value the number
     * @return the number, as JSON
     */
    static String number(double value) {
        if (Double.isNaN(value) || Double.isInfinite(value)) {
            return "0";
        }
        if (value == Math.rint(value) && Math.abs(value) < 1e15) {
            return Long.toString((long) value);
        }
        return String.format(java.util.Locale.ROOT, "%.6f", value);
    }
}
