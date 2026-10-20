package org.lwjgl.sdl;

/**
 * A stand-in for the class of LWJGL that puts the frame on the screen
 * through SDL.
 *
 * <p>The real class cannot be loaded outside of a game — it needs the
 * native library of SDL — so the test uses this one. It has the same
 * name, the same package and the same shape: a static method that
 * takes a window and says whether the frame was put on the screen.
 *
 * <p>Unlike the method of GLFW, this one gives an answer back, and the
 * answer has to survive the weaving: the game reads it.
 */
public final class SDLVideo {

    /** The number of times the frame was put on the screen. */
    public static int swaps;

    private SDLVideo() {
    }

    /**
     * Put the frame on the screen.
     *
     * @param window the window
     * @return true, as SDL says
     */
    public static boolean SDL_GL_SwapWindow(long window) {
        swaps++;
        // A little work, so that the frame takes a measurable amount of
        // time and the test can tell one frame from another.
        long sum = 0L;
        for (int i = 0; i < 1000; i++) {
            sum += i;
        }
        if (sum == Long.MIN_VALUE) {
            throw new IllegalStateException("never");
        }
        return true;
    }
}
