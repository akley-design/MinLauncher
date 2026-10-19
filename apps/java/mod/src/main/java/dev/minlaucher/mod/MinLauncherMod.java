package dev.minlaucher.mod;

import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The entry point of the optimisation mod.
 *
 * <p>The mod does two things when it starts:
 *
 * <ol>
 *   <li>it reads the settings the launcher wrote, and keeps them;</li>
 *   <li>it tells the game what to render, so that the expensive parts
 *       are left out.</li>
 * </ol>
 *
 * <p>Nothing here is allowed to stop the game from starting. A mod that
 * fails is a game that does not start, and the player has no way to
 * tell that the mod was the reason.
 */
public final class MinLauncherMod implements ClientModInitializer {

    /** The name of the mod, as the log knows it. */
    public static final String MOD_ID = "minlaucher-optimisations";

    /** The log of the mod. */
    private static final Logger LOGGER = LoggerFactory.getLogger(MOD_ID);

    /**
     * The settings the mod works with.
     *
     * <p>They are read the first time they are asked for, and not
     * when the mod starts: the game reads its options before the mod
     * is told to start, so the settings have to be there when the
     * first of them is wanted.
     */
    private static volatile OptimisationSettings settings;

    private MinLauncherMod() {
    }

    /**
     * The settings the mod works with.
     *
     * <p>The mixin asks for them, and they must be the same
     * everywhere: a mod that renders one thing and measures another is
     * worse than one that does nothing.
     *
     * @return the settings
     */
    public static OptimisationSettings settings() {
        OptimisationSettings current = settings;
        if (current == null) {
            synchronized (MinLauncherMod.class) {
                current = settings;
                if (current == null) {
                    current = read();
                    settings = current;
                }
            }
        }
        return current;
    }

    /**
     * Read the settings the launcher wrote.
     *
     * <p>A file that is not there, or that cannot be read, gives the
     * balanced preset: the game must start even when the launcher
     * could not write the settings.
     *
     * @return the settings
     */
    private static OptimisationSettings read() {
        try {
            OptimisationSettings read = new SettingsStore(java.nio.file.Paths.get(".")).load();
            LOGGER.info("the mod started with the preset `{}`", read.getPreset());
            return read;
        } catch (RuntimeException failure) {
            LOGGER.warn("the settings of the mod could not be read: {}", failure.toString());
            return OptimisationSettings.of(OptimisationSettings.BALANCED);
        }
    }

    @Override
    public void onInitializeClient() {
        // The settings are read when they are first wanted, which may
        // be before this is called. Reading them here as well only
        // makes the log say it earlier.
        settings();
    }
}
