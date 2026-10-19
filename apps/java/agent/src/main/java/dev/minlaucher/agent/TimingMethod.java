package dev.minlaucher.agent;

import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.commons.AdviceAdapter;
import org.objectweb.asm.commons.Method;

/**
 * The body that is woven around the call that puts the frame on the
 * screen.
 *
 * <p>The method being woven is an ordinary Java method, so the timing
 * is woven straight into it: the moment the frame went to the screen
 * is taken when the method is left, and it is handed to the counter.
 *
 * <p>What is measured is the moment the frame went to the screen, and
 * not the time the call took. The call itself is nearly free — it
 * hands the finished picture to the driver and returns — while the
 * frame time is the whole interval between two pictures, and the
 * number the game is judged by is the interval. The counter works
 * the interval out of the moments, so that the two frames that are
 * measured are the ones that were really on the screen.
 *
 * <p>The method is left alone in every other respect: its name, its
 * signature and its result are the same, so the game cannot tell that
 * anything happened.
 */
final class TimingMethod extends AdviceAdapter {

    /** The class that holds the counter, in the internal form. */
    private static final String COUNTER = "dev/minlaucher/agent/FrameCounter";

    /** The name of the method of {@link #COUNTER} that records a frame. */
    private static final String RECORD = "recordFrame";

    /**
     * Create the woven method.
     *
     * @param delegate the visitor of the method being woven
     * @param access the access flags of the method being woven
     * @param name the name of the method being woven
     * @param descriptor the descriptor of the method being woven
     */
    TimingMethod(
            MethodVisitor delegate,
            int access,
            String name,
            String descriptor) {
        super(Opcodes.ASM9, delegate, access, name, descriptor);
    }

    @Override
    protected void onMethodExit(int opcode) {
        // The frame is recorded on every way out of the method — the
        // normal one and the exceptional one alike. A frame that was
        // drawn and then failed is still a frame.
        //
        //   FrameCounter.recordFrame(System.nanoTime());
        //
        // What is passed is the moment the frame was put on the
        // screen, and not the time the call took: the frame time is
        // the interval between two frames, and the counter works it
        // out from these moments.
        invokeStatic(Type.getType(System.class), new Method("nanoTime", "()J"));
        invokeStatic(Type.getType(FrameCounter.class), new Method(RECORD, "(J)V"));
    }
}
