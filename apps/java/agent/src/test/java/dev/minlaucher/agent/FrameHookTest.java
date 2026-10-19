package dev.minlaucher.agent;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.io.InputStream;
import org.junit.jupiter.api.Test;

/**
 * The tests of the weaving of the frame counter.
 */
class FrameHookTest {

    /** The name of the class that is woven, in the internal form. */
    private static final String GLFW = "org/lwjgl/glfw/GLFW";

    /**
     * The bytes of the class that is woven.
     *
     * @return the bytes of the class file
     * @throws IOException when the class cannot be read
     */
    private static byte[] classBytes() throws IOException {
        try (InputStream stream = FrameHookTest.class.getClassLoader()
                .getResourceAsStream(GLFW + ".class")) {
            assertNotNull(stream, "the class was not found");
            byte[] bytes = new byte[stream.available()];
            int read = 0;
            while (read < bytes.length) {
                int count = stream.read(bytes, read, bytes.length - read);
                if (count < 0) {
                    break;
                }
                read += count;
            }
            return bytes;
        }
    }

    /**
     * The class that is woven, loaded in a class loader of its own.
     *
     * <p>The class is loaded twice: once by the loader of the test,
     * and once by the loader below, from the woven bytes. The second
     * one is the one that counts the frames.
     */
    private static final class WovenLoader extends ClassLoader {

        /** The bytes of the woven class. */
        private final byte[] bytes;

        /** The name of the woven class, with dots. */
        private final String name;

        WovenLoader(byte[] bytes, String internalName) {
            super(FrameHookTest.class.getClassLoader());
            this.bytes = bytes;
            this.name = internalName.replace('/', '.');
        }

        @Override
        protected Class<?> findClass(String requested) throws ClassNotFoundException {
            if (!requested.equals(name)) {
                return super.findClass(requested);
            }
            return defineClass(requested, bytes, 0, bytes.length);
        }

        @Override
        protected Class<?> loadClass(String requested, boolean resolve)
                throws ClassNotFoundException {
            if (!requested.equals(name)) {
                return super.loadClass(requested, resolve);
            }
            Class<?> found = findLoadedClass(requested);
            if (found == null) {
                found = findClass(requested);
            }
            if (resolve) {
                resolveClass(found);
            }
            return found;
        }
    }

    @Test
    void a_class_that_is_not_the_one_of_the_library_is_left_alone() throws IOException {
        FrameHook hook = new FrameHook(FrameCounter.get());

        byte[] result = hook.transform(
                null,
                "net/minecraft/client/Minecraft",
                null,
                null,
                classBytes());

        // The class is not the one that puts the frame on the screen,
        // so it is not touched at all.
        assertNull(result);
    }

    @Test
    void the_woven_class_still_has_the_method_of_the_library() throws Exception {
        FrameHook hook = new FrameHook(FrameCounter.get());

        byte[] result = hook.transform(null, GLFW, null, null, classBytes());

        assertNotNull(result, "the class was not woven");

        Class<?> woven = new WovenLoader(result, GLFW).loadClass("org.lwjgl.glfw.GLFW");

        // The method keeps its name and its signature, so the game
        // cannot tell that anything happened.
        assertNotNull(woven.getMethod("glfwSwapBuffers", long.class));
        assertEquals(void.class, woven.getMethod("glfwSwapBuffers", long.class).getReturnType());
    }

    @Test
    void a_frame_that_was_drawn_is_counted() throws Exception {
        FrameCounter counter = FrameCounter.get();
        FrameHook hook = new FrameHook(counter);
        counter.reset();

        byte[] result = hook.transform(null, GLFW, null, null, classBytes());
        assertNotNull(result, "the class was not woven");

        Class<?> woven = new WovenLoader(result, GLFW).loadClass("org.lwjgl.glfw.GLFW");
        java.lang.reflect.Method swap =
                woven.getMethod("glfwSwapBuffers", long.class);

        // The first frame has no predecessor, so it is not measured:
        // the time before the game started says nothing about how the
        // game runs.
        swap.invoke(null, 0L);
        assertEquals(0, counter.statistics().count());

        // The second one is the first frame that has an interval.
        swap.invoke(null, 0L);
        assertEquals(1, counter.statistics().count());
    }

    @Test
    void the_time_of_the_frame_is_measured() throws Exception {
        FrameCounter counter = FrameCounter.get();
        FrameHook hook = new FrameHook(counter);
        counter.reset();

        byte[] result = hook.transform(null, GLFW, null, null, classBytes());
        assertNotNull(result, "the class was not woven");

        Class<?> woven = new WovenLoader(result, GLFW).loadClass("org.lwjgl.glfw.GLFW");
        java.lang.reflect.Method swap =
                woven.getMethod("glfwSwapBuffers", long.class);

        // Two frames, so that there is an interval between them.
        swap.invoke(null, 0L);
        swap.invoke(null, 0L);

        // The frame took a measurable amount of time: the stand-in
        // does a thousand additions, which is more than the clock can
        // resolve.
        double frameTime = counter.statistics().averageFrametime();
        assertTrue(frameTime > 0.0, "the frame took no time: " + frameTime);
    }

    @Test
    void the_class_of_the_library_is_not_woven_twice() throws Exception {
        FrameCounter counter = FrameCounter.get();
        FrameHook hook = new FrameHook(counter);
        counter.reset();

        byte[] result = hook.transform(null, GLFW, null, null, classBytes());
        assertNotNull(result, "the class was not woven");

        Class<?> woven = new WovenLoader(result, GLFW).loadClass("org.lwjgl.glfw.GLFW");
        java.lang.reflect.Method swap =
                woven.getMethod("glfwSwapBuffers", long.class);

        swap.invoke(null, 0L);
        swap.invoke(null, 0L);

        // Two frames make one interval, and one interval is one count.
        // The method is not nested in itself, so nothing is counted
        // twice.
        assertEquals(1, counter.statistics().count());
    }
}
