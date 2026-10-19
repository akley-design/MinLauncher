package dev.minlaucher.mod;

import net.minecraft.client.option.CloudRenderMode;
import net.minecraft.client.option.GameOptions;
import net.minecraft.client.option.GraphicsMode;
import net.minecraft.client.option.SimpleOption;
import net.minecraft.client.render.ChunkBuilderMode;
import net.minecraft.particle.ParticlesMode;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Puts the settings of the launcher into the options of the game.
 *
 * <p>This is the whole of the mod. It does not draw anything itself
 * and it does not touch the renderers: it changes the options the
 * game already has, and the game then leaves the expensive work out
 * by itself. That is why the mod is small, and why it keeps working
 * when the game changes the way it draws things — the mod does not
 * know how the game draws, only what it is asked to draw.
 *
 * <p>The options of the game are private, and the game gives them out
 * through getters that return the option itself. That is what this
 * class uses: it holds an option, and it changes the value inside it.
 */
public final class OptionApplier {

    /** The log of the mod. */
    private static final Logger LOGGER = LoggerFactory.getLogger(MinLauncherMod.MOD_ID);

    private OptionApplier() {
    }

    /**
     * Put the settings into the options of the game.
     *
     * <p>Nothing here is allowed to stop the game from starting: a
     * setting that cannot be applied is a setting that is left as it
     * was.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    public static void apply(GameOptions gameOptions, OptimisationSettings settings) {
        if (!settings.isEnabled()) {
            LOGGER.info("the mod is off, and the options of the game are left alone");
            return;
        }

        try {
            applyClouds(gameOptions, settings);
            applyGraphics(gameOptions, settings);
            applyParticles(gameOptions, settings);
            applySwitches(gameOptions, settings);
            applyDistances(gameOptions, settings);
            applyFrameLimit(gameOptions, settings);
        } catch (RuntimeException failure) {
            // A version of the game that does not have one of the
            // settings must not break the game.
            LOGGER.warn("the options of the game could not be changed: {}", failure.toString());
        }
    }

    /**
     * Change the clouds.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applyClouds(GameOptions gameOptions, OptimisationSettings settings) {
        CloudRenderMode wanted = settings.hasClouds() ? CloudRenderMode.FANCY : CloudRenderMode.OFF;
        set(gameOptions.getCloudRenderMode(), wanted, "the clouds");
    }

    /**
     * Change the graphics.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applyGraphics(GameOptions gameOptions, OptimisationSettings settings) {
        GraphicsMode wanted = settings.hasFancyGraphics() ? GraphicsMode.FANCY : GraphicsMode.FAST;
        set(gameOptions.getGraphicsMode(), wanted, "the graphics");
    }

    /**
     * Change the particles.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applyParticles(GameOptions gameOptions, OptimisationSettings settings) {
        ParticlesMode wanted = settings.hasParticles() ? ParticlesMode.ALL : ParticlesMode.MINIMAL;
        set(gameOptions.getParticles(), wanted, "the particles");
    }

    /**
     * Change the switches that are on or off.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applySwitches(GameOptions gameOptions, OptimisationSettings settings) {
        set(gameOptions.getEnableVsync(), settings.hasVsync(), "the vertical sync");
        set(gameOptions.getAo(), settings.hasAmbientOcclusion(), "the ambient occlusion");
        set(gameOptions.getEntityShadows(), settings.hasEntityShadows(), "the shadows");
        set(gameOptions.getHideLightningFlashes(), !settings.hasWeather(), "the lightning");
        set(
                gameOptions.getChunkBuilderMode(),
                settings.hasBackgroundChunks() ? ChunkBuilderMode.NEARBY : ChunkBuilderMode.NONE,
                "the chunks in the background");
    }

    /**
     * Change how far the world reaches.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applyDistances(GameOptions gameOptions, OptimisationSettings settings) {
        if (settings.getRenderDistance() > 0) {
            set(gameOptions.getViewDistance(), settings.getRenderDistance(), "the view distance");
        }
        if (settings.getSimulationDistance() > 0) {
            set(
                    gameOptions.getSimulationDistance(),
                    settings.getSimulationDistance(),
                    "the simulation distance");
        }
    }

    /**
     * Change the cap of the frames.
     *
     * @param gameOptions the options of the game
     * @param settings the settings of the launcher
     */
    private static void applyFrameLimit(GameOptions gameOptions, OptimisationSettings settings) {
        if (settings.getFrameLimit() > 0) {
            set(gameOptions.getMaxFps(), settings.getFrameLimit(), "the frame cap");
        }
    }

    /**
     * Put a value into an option of the game.
     *
     * @param option the option of the game
     * @param value the value
     * @param name the name of the setting, for the log
     * @param <T> the type of the value
     */
    private static <T> void set(SimpleOption<T> option, T value, String name) {
        T current = option.getValue();
        if (value == null || value.equals(current)) {
            return;
        }
        option.setValue(value);
        LOGGER.info("{}: {} -> {}", name, current, value);
    }
}
