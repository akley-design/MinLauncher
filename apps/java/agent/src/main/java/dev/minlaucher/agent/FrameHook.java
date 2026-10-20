package dev.minlaucher.agent;

import java.lang.instrument.ClassFileTransformer;
import java.security.ProtectionDomain;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.commons.AdviceAdapter;

/**
 * Counts the frames of the game.
 *
 * <p>The frame is put on the screen by a call to LWJGL, and that call
 * is the same in every version of the game:
 *
 * <ul>
 *   <li>{@code org.lwjgl.glfw.GLFW.gfwSwapBuffers(long)} — LWJGL 3,
 *       the game of 1.17 and later;</li>
 *   <li>{@code org.lwjgl.opengl.Display.update()} — LWJGL 2, the game up
 *       to 1.12.2;</li>
 *   <li>{@code org.lwjgl.sdl.SDLVideo.SDL_GL_SwapWindow(long)} — LWJGL 3
 *       with SDL, the game of 26.1 and later. Unlike the two others it
 *       says whether the window was swapped.</li>
 * </ul>
 *
 * <p>Mojang does not obfuscate the libraries of the game, so these
 * methods can be named. The name of the class of the game itself
 * cannot: in the official jar it is called something like {@code fij},
 * and it differs from version to version.
 *
 * <p>Both methods are ordinary Java methods, not native ones — LWJGL
 * keeps the address of the real function in a field and calls it
 * through a pointer. That is what makes this possible: the body of the
 * method can be measured, without renaming anything and without
 * touching the class of the game at all.
 *
 * <p>The names of the LWJGL classes are the only thing this class
 * knows about the game, and they are also the only thing that can
 * change: a version of the game that uses a different library would
 * simply not be counted, and the report would say so.
 */
final class FrameHook implements ClassFileTransformer {

    /** The class that puts the frame on the screen, LWJGL 3. */
    private static final String GLFW = "org/lwjgl/glfw/GLFW";

    /** The method of {@link #GLFW} that puts the frame on the screen. */
    private static final String SWAP_BUFFERS = "glfwSwapBuffers";

    /** The class that puts the frame on the screen, LWJGL 2. */
    private static final String DISPLAY = "org/lwjgl/opengl/Display";

    /** The method of {@link #DISPLAY} that puts the frame on the screen. */
    private static final String UPDATE = "update";

    /** The class that puts the frame on the screen, LWJGL 3 with SDL. */
    private static final String SDL_VIDEO = "org/lwjgl/sdl/SDLVideo";

    /** The method of {@link #SDL_VIDEO} that puts the frame on the screen. */
    private static final String SDL_SWAP_WINDOW = "SDL_GL_SwapWindow";

    /** The counter of the frames. */
    private final FrameCounter counter;

    /**
     * Create a hook.
     *
     * @param counter the counter of the frames
     */
    FrameHook(FrameCounter counter) {
        this.counter = counter;
    }

    @Override
    public byte[] transform(
            ClassLoader loader,
            String className,
            Class<?> classBeingRedefined,
            ProtectionDomain protectionDomain,
            byte[] classfileBuffer) {
        if (className == null) {
            return null;
        }

        boolean isGlfw = GLFW.equals(className);
        boolean isDisplay = DISPLAY.equals(className);
        boolean isSdl = SDL_VIDEO.equals(className);
        if (!isGlfw && !isDisplay && !isSdl) {
            return null;
        }

        try {
            ClassReader reader = new ClassReader(classfileBuffer);
            ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_FRAMES);

            ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
                @Override
                public MethodVisitor visitMethod(
                        int access,
                        String name,
                        String descriptor,
                        String signature,
                        String[] exceptions) {
                    MethodVisitor delegate =
                            super.visitMethod(access, name, descriptor, signature, exceptions);

                    if (!isTarget(access, name, descriptor, isGlfw, isSdl)) {
                        return delegate;
                    }

                    return new TimingMethod(delegate, access, name, descriptor);
                }
            };

            // The frames are expanded, because the weaving needs to
            // know the local variables of every point of the method:
            // `AdviceAdapter` sorts them, and it only works on
            // expanded frames.
            reader.accept(visitor, ClassReader.EXPAND_FRAMES);
            return writer.toByteArray();
        } catch (Throwable failure) {
            // A transformer that throws would make the class fail to
            // load, and the game would not start at all. The failure is
            // reported in the report, and the game runs without being
            // measured.
            counter.reportFailure("cannot weave " + className + ": " + failure);
            return null;
        }
    }

    /**
     * Whether the method is the one that puts the frame on the screen.
     *
     * @param access the access flags
     * @param name the name of the method
     * @param descriptor the descriptor of the method
     * @param isGlfw whether the class is the one of LWJGL 3
     * @param isSdl whether the class is the one of LWJGL 3 with SDL
     * @return true when the method has to be woven
     */
    private static boolean isTarget(
            int access, String name, String descriptor, boolean isGlfw, boolean isSdl) {
        // The method must be static, and it must not be native: a
        // native method has no body, and there would be nothing to
        // measure.
        if ((access & Opcodes.ACC_STATIC) == 0) {
            return false;
        }
        if ((access & Opcodes.ACC_NATIVE) != 0) {
            return false;
        }
        if ((access & Opcodes.ACC_ABSTRACT) != 0) {
            return false;
        }

        if (isSdl) {
            // SDL_GL_SwapWindow(long window): boolean
            //
            // Unlike the two others, this one says whether the window
            // was swapped. The answer is left alone: the frame is
            // counted when the method is left, and the weaving does
            // not change what the method gives back.
            return SDL_SWAP_WINDOW.equals(name)
                    && Type.getReturnType(descriptor).getSort() == Type.BOOLEAN
                    && Type.getArgumentTypes(descriptor).length == 1;
        }

        if (Type.getReturnType(descriptor).getSort() != Type.VOID) {
            return false;
        }

        if (isGlfw) {
            // glfwSwapBuffers(long window)
            return SWAP_BUFFERS.equals(name)
                    && Type.getArgumentTypes(descriptor).length == 1;
        }

        // update()
        //
        // The method `update(boolean)` is not woven as well: `update()`
        // calls it, and a frame that is counted twice would halve the
        // frame rate that is reported.
        return UPDATE.equals(name)
                && Type.getArgumentTypes(descriptor).length == 0;
    }
}
