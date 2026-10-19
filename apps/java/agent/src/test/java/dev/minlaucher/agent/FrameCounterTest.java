package dev.minlaucher.agent;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * The tests of the counter of the frames.
 */
class FrameCounterTest {

    @Test
    void the_first_frame_is_not_measured() {
        FrameCounter counter = FrameCounter.get();
        counter.reset();

        // The first frame has no predecessor, so there is no interval
        // to measure: the time before the game started says nothing
        // about how the game runs.
        FrameCounter.recordFrame(1_000_000L);

        assertEquals(0, counter.statistics().count());
    }

    @Test
    void the_time_between_two_frames_is_measured() {
        FrameCounter counter = FrameCounter.get();
        counter.reset();

        // Two frames, sixteen milliseconds apart.
        FrameCounter.recordFrame(1_000_000L);
        FrameCounter.recordFrame(17_000_000L);

        assertEquals(1, counter.statistics().count());
        assertEquals(16.0, counter.statistics().averageFrametime(), 1e-9);
    }

    @Test
    void the_frames_that_go_backwards_are_ignored() {
        FrameCounter counter = FrameCounter.get();
        counter.reset();

        // A frame cannot be put on the screen before the one before
        // it. Such a frame is dropped, because a negative interval
        // would make the average of the frame times a lie.
        FrameCounter.recordFrame(1_000_000L);
        FrameCounter.recordFrame(500_000L);
        FrameCounter.recordFrame(17_000_000L);

        assertEquals(1, counter.statistics().count());
    }
}
