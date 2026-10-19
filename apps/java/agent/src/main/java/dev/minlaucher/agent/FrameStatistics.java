package dev.minlaucher.agent;

/**
 * The frame times of a run, and the numbers that come out of them.
 *
 * <p>The metrics M5 and M6 are the average frame rate and the
 * ninetieth-ninth percentile of the frame time. Both are computed from
 * the same list of frame times, so the list is kept and the two
 * numbers are derived from it — the numbers are never measured
 * separately, because two measurements of the same thing disagree.
 *
 * <p>The list is not unbounded: a game that runs for six hours would
 * otherwise keep every frame in memory. The oldest frames are dropped
 * once the list is full, which makes the numbers describe the last
 * minutes of the run rather than the whole of it. That is the right
 * thing to measure anyway: the first minutes of a game are slower than
 * the rest, and they would drag the average down for reasons that have
 * nothing to do with the launcher.
 */
final class FrameStatistics {

    /**
     * How many frames are kept.
     *
     * <p>At sixty frames a second this is two minutes of the game, and
     * at four hundred it is ten seconds. The larger number would cost
     * more memory for no gain: the percentile of the last two minutes
     * is the same as the percentile of the last ten seconds for any
     * frame rate that is not a slideshow.
     */
    static final int CAPACITY = 7_200;

    /** The frame times, in nanoseconds, oldest first. */
    private final long[] times;

    /** The number of frames in {@link #times}. */
    private int count;

    /** The index of the next frame to write. */
    private int next;

    FrameStatistics() {
        this.times = new long[CAPACITY];
    }

    /**
     * Forget all the frames.
     *
     * <p>The tests need this: the counter of the process is one, and
     * the tests share it, so they have to be able to start from a
     * clean slate. Nothing else calls it.
     */
    synchronized void clear() {
        count = 0;
        next = 0;
    }

    /**
     * Add a frame.
     *
     * @param nanoseconds the time the frame took
     */
    synchronized void add(long nanoseconds) {
        times[next] = nanoseconds;
        next = (next + 1) % CAPACITY;
        if (count < CAPACITY) {
            count++;
        }
    }

    /**
     * The number of frames that were measured.
     */
    synchronized int count() {
        return count;
    }

    /**
     * The average frame rate, in frames a second.
     *
     * @return the frame rate, or zero when nothing was measured
     */
    synchronized double averageFps() {
        if (count == 0) {
            return 0.0;
        }
        double total = 0.0;
        for (int i = 0; i < count; i++) {
            total += times[i];
        }
        double mean = total / count;
        if (mean <= 0.0) {
            return 0.0;
        }
        return 1_000_000_000.0 / mean;
    }

    /**
     * The average frame time, in milliseconds.
     */
    synchronized double averageFrametime() {
        if (count == 0) {
            return 0.0;
        }
        long total = 0L;
        for (int i = 0; i < count; i++) {
            total += times[i];
        }
        return total / (double) count / 1_000_000.0;
    }

    /**
     * The percentile of the frame times, in milliseconds.
     *
     * <p>The percentile is taken over the sorted list of the frame
     * times. The list is small enough — a few thousand entries — that
     * sorting it costs less than the game does in a single second, and
     * a copy is sorted so that the order of the frames is kept.
     *
     * @param percentile the percentile, from 0 to 100
     * @return the percentile, in milliseconds
     */
    synchronized double percentileFrametime(double percentile) {
        if (count == 0) {
            return 0.0;
        }
        long[] sorted = new long[count];
        System.arraycopy(times, 0, sorted, 0, count);
        java.util.Arrays.sort(sorted);

        int index = (int) Math.ceil(percentile / 100.0 * count) - 1;
        if (index < 0) {
            index = 0;
        }
        if (index >= count) {
            index = count - 1;
        }
        return sorted[index] / 1_000_000.0;
    }

    /**
     * The frame time of the slowest frames, in milliseconds.
     *
     * <p>This is the "one per cent low" that the metric M6 talks about:
     * the average of the slowest one per cent of the frames. It is
     * the number that shows how the game feels when it stutters.
     */
    synchronized double onePercentLow() {
        if (count == 0) {
            return 0.0;
        }
        long[] sorted = new long[count];
        System.arraycopy(times, 0, sorted, 0, count);
        java.util.Arrays.sort(sorted);

        // The slowest one per cent of the frames. At least one frame is
        // always included, otherwise a run of a handful of frames would
        // have no low at all.
        int taken = Math.max(1, count / 100);
        long total = 0L;
        for (int i = count - taken; i < count; i++) {
            total += sorted[i];
        }
        return total / (double) taken / 1_000_000.0;
    }

    /**
     * The frame times in milliseconds, oldest first.
     *
     * <p>This is what goes into the report, so that the launcher can
     * keep the whole run and not only its summary.
     *
     * <p>The buffer is a ring: once it is full, the oldest frame is
     * the one the next frame will be written over. The frames are read
     * from there, so that they come out in the order they happened in
     * and not in the order they happen to sit in the array.
     */
    synchronized double[] frameTimesMillis() {
        double[] result = new double[count];
        int start = (count == CAPACITY) ? next : 0;
        for (int i = 0; i < count; i++) {
            result[i] = times[(start + i) % CAPACITY] / 1_000_000.0;
        }
        return result;
    }
}
