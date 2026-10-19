package dev.minlaucher.agent;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * The tests of the statistics of the frames.
 */
class FrameStatisticsTest {

    @Test
    void an_empty_run_has_no_numbers() {
        FrameStatistics statistics = new FrameStatistics();

        assertEquals(0, statistics.count());
        assertEquals(0.0, statistics.averageFps(), 1e-9);
        assertEquals(0.0, statistics.averageFrametime(), 1e-9);
        assertEquals(0.0, statistics.percentileFrametime(99.0), 1e-9);
        assertEquals(0.0, statistics.onePercentLow(), 1e-9);
    }

    @Test
    void the_frame_rate_is_the_inverse_of_the_frame_time() {
        FrameStatistics statistics = new FrameStatistics();

        // Sixty frames, each of them taking 16.666 milliseconds: about
        // sixty frames a second.
        for (int i = 0; i < 60; i++) {
            statistics.add(16_666_666L);
        }

        assertEquals(60, statistics.count());
        assertEquals(60.0, statistics.averageFps(), 0.1);
        assertEquals(16.666, statistics.averageFrametime(), 0.001);
    }

    @Test
    void the_percentile_of_the_frame_times_is_the_right_one() {
        FrameStatistics statistics = new FrameStatistics();

        // A hundred frames, from 1 millisecond to 100 milliseconds.
        for (int i = 1; i <= 100; i++) {
            statistics.add(i * 1_000_000L);
        }

        assertEquals(50.0, statistics.percentileFrametime(50.0), 1e-6);
        assertEquals(95.0, statistics.percentileFrametime(95.0), 1e-6);
        assertEquals(99.0, statistics.percentileFrametime(99.0), 1e-6);
    }

    @Test
    void the_one_per_cent_low_is_the_average_of_the_slowest_frames() {
        FrameStatistics statistics = new FrameStatistics();

        // A hundred frames, from 1 millisecond to 100 milliseconds. The
        // slowest one per cent is the frame of 100 milliseconds.
        for (int i = 1; i <= 100; i++) {
            statistics.add(i * 1_000_000L);
        }

        assertEquals(100.0, statistics.onePercentLow(), 1e-6);
    }

    @Test
    void a_run_of_a_few_frames_still_has_a_low() {
        FrameStatistics statistics = new FrameStatistics();

        statistics.add(10_000_000L);
        statistics.add(20_000_000L);

        // There is no one per cent of two frames, but the low must not
        // be empty: it is the slowest frame.
        assertEquals(20.0, statistics.onePercentLow(), 1e-6);
    }

    @Test
    void the_oldest_frames_are_dropped_when_the_buffer_is_full() {
        FrameStatistics statistics = new FrameStatistics();

        // One more frame than the buffer holds.
        for (int i = 0; i < FrameStatistics.CAPACITY + 1; i++) {
            statistics.add(1_000_000L);
        }

        assertEquals(FrameStatistics.CAPACITY, statistics.count());
    }

    @Test
    void the_frame_times_come_out_in_the_order_they_happened_in() {
        FrameStatistics statistics = new FrameStatistics();

        // Fill the buffer, so that it wraps around, and then add one
        // more frame. Every frame takes a millisecond more than the
        // one before it, so that the order of the frames is the order
        // of their length.
        for (int i = 0; i < FrameStatistics.CAPACITY; i++) {
            statistics.add((i + 1) * 1_000_000L);
        }
        statistics.add((FrameStatistics.CAPACITY + 1L) * 1_000_000L);

        double[] times = statistics.frameTimesMillis();

        assertEquals(FrameStatistics.CAPACITY, times.length);
        // The oldest frame is the one that was added first, and the
        // first one was dropped when the buffer wrapped around.
        assertEquals(2.0, times[0], 1e-6);
        // The newest one is the last.
        assertEquals(
                (FrameStatistics.CAPACITY + 1.0),
                times[times.length - 1],
                1e-6);
        for (int i = 1; i < times.length; i++) {
            assertTrue(times[i] > times[i - 1], "the frames are out of order at " + i);
        }
    }
}
