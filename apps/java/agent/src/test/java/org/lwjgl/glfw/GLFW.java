package org.lwjgl.glfw;

/**
 * A stand-in for the class of LWJGL that puts the frame on the screen.
 *
 * <p>The real class cannot be loaded outside of a game — it needs the
 * native library of LWJGL — so the test uses this one. It has the same
 * name, the same package and the same shape: a static method that
 * takes a window and returns nothing.
 *
 * <p>The real method is not native either. It keeps the address of the
 * real function in a field and calls it through a pointer, and that is
 * exactly what makes the weaving possible: the body of the method can
 * be measured.
 */
public final class GLFW {

    /** The number of times the frame was put on the screen. */
    public static int swaps;

    private GLFW() {
    }

    /**
     * Put the frame on the screen.
     *
     * @param window the window
     */
    public static void glfwSwapBuffers(long window) {
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
    }
}
