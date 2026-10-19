package dev.minlaucher.agent;

/**
 * The counter of the frames.
 *
 * <p>The woven code calls {@link #recordFrame(long)} directly — it is
 * woven into the game, and the game must not depend on anything that
 * can fail to load, so the method is static and the class has no
 * dependencies of its own.
 *
 * <p>The counter is a singleton, because the woven code cannot be given
 * a reference to anything: it is woven into a class of the game, and
 * the game knows nothing about the agent.
 */
public final class FrameCounter {

    /** The one counter of the process. */
    private static final FrameCounter INSTANCE = new FrameCounter();

    /** The frame times. */
    private final FrameStatistics statistics = new FrameStatistics();

    /** The time the agent was attached, in nanoseconds. */
    private final long attachedAt = System.nanoTime();

    /** The time the previous frame was put on the screen, in nanoseconds. */
    private long lastFrameAt;

    /** The failures that happened while the agent worked. */
    private final java.util.List<String> failures =
            java.util.Collections.synchronizedList(new java.util.ArrayList<String>());

    /**
     * Record a frame.
     *
     * <p>This method is called from the woven code, once per frame.
     * It must stay short: it is on the path of every frame of the
     * game, and a slow measurement would show up in the very numbers
     * it tries to measure.
     *
     * <p>What is recorded is the time <em>between</em> two frames, and
     * not the time the call took. The call itself is nearly free — it
     * hands the finished picture to the driver — while the frame time
     * is the whole interval between two pictures. The metric M6 talks
     * about the frame time, and it is the interval that is measured.
     *
     * <p>The very first frame has no predecessor, so it is not
     * measured: the time before the game started says nothing about
     * how the game runs.
     *
     * @param now the time the frame was put on the screen
     */
    public static void recordFrame(long now) {
        INSTANCE.add(now);
    }

    /**
     * Record a frame, and the time it was put on the screen.
     *
     * @param now the time the frame was put on the screen
     */
    private synchronized void add(long now) {
        long previous = lastFrameAt;
        lastFrameAt = now;
        if (previous != 0L) {
            long delta = now - previous;
            if (delta > 0L) {
                statistics.add(delta);
            }
        }
    }

    private FrameCounter() {
    }

    /**
     * Forget everything.
     *
     * <p>The counter of the process is one, and the tests share it, so
     * they have to be able to start from a clean slate. Nothing else
     * calls this: a game is attached to once, and the counter starts
     * empty on its own.
     */
    synchronized void reset() {
        lastFrameAt = 0L;
        statistics.clear();
    }

    /**
     * The counter of the process.
     *
     * @return the counter
     */
    public static FrameCounter get() {
        return INSTANCE;
    }

    /**
     * The frame times of the run.
     *
     * @return the statistics
     */
    public FrameStatistics statistics() {
        return statistics;
    }

    /**
     * How long the agent has been attached, in nanoseconds.
     *
     * @return the time since the agent was attached
     */
    public long attachedAt() {
        return attachedAt;
    }

    /**
     * Report that something went wrong.
     *
     * @param message the description of the failure
     */
    public void reportFailure(String message) {
        failures.add(message);
    }

    /**
     * The failures that happened while the agent worked.
     *
     * @return the descriptions of the failures
     */
    public java.util.List<String> failures() {
        synchronized (failures) {
            return new java.util.ArrayList<String>(failures);
        }
    }
}
