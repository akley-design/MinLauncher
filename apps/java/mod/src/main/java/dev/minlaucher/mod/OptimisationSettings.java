package dev.minlaucher.mod;

/**
 * What the mod changes, and how much of it.
 *
 * <p>Every optimisation of the mod is a decision that can be made
 * without the game: the settings are read from a file, and the classes
 * that hold the decisions do not know anything about Minecraft. That is
 * what makes them testable — a test can ask what the mod does with a
 * slow machine and check the answer, without starting a game.
 *
 * <p>The presets are the two the launcher offers:
 *
 * <ul>
 *   <li>{@code balanced} — the game looks the same and runs faster;
 *       nothing that a player would notice is turned off;</li>
 *   <li>{@code fast} — the game runs as fast as it can, and looks
 *       slightly worse: the expensive effects are off.</li>
 * </ul>
 *
 * <p>Every value here is a value the game itself has, in the version
 * the mod is built against. The mod does not invent settings: it sets
 * the ones of the game, so the game then leaves the expensive work out
 * by itself, without any of the renderers having to know about the
 * mod.
 *
 * <p>The fields are open to the package on purpose: the store that
 * reads the file of the launcher is next door, and it fills them in.
 * Everything outside the package reads them through the getters.
 */
public final class OptimisationSettings {

    /** The name of the preset that keeps the look of the game. */
    public static final String BALANCED = "balanced";

    /** The name of the preset that puts the speed before the look. */
    public static final String FAST = "fast";

    /** The name of the preset that turns everything off. */
    public static final String OFF = "off";

    /**
     * The name of the preset.
     *
     * <p>The default is the balanced one: a player who installs the
     * mod expects the game to be faster, and not to look different.
     */
    String preset = BALANCED;

    /**
     * Whether the clouds are drawn.
     *
     * <p>The clouds are the single most expensive thing in the game
     * for what they add: a full extra pass over the sky, every frame.
     */
    boolean clouds = true;

    /**
     * Whether the rain and the snow are drawn.
     *
     * <p>Same as the clouds: a whole extra pass, and the rain is
     * drawn again for every block of the world.
     */
    boolean weather = true;

    /**
     * Whether the particles are drawn.
     */
    boolean particles = true;

    /**
     * Whether the shadows of the entities are drawn.
     *
     * <p>Every entity in view leaves a shadow on the ground, and every
     * shadow is a small pass of the renderer.
     */
    boolean entityShadows = true;

    /**
     * Whether the ambient occlusion is used.
     *
     * <p>The ambient occlusion is what makes the corners of the blocks
     * dark. It is drawn per block, per face.
     */
    boolean ambientOcclusion = true;

    /**
     * Whether the fancy graphics are used.
     *
     * <p>The fancy graphics of the game cost a lot: the leaves are
     * transparent, the water is wavy, the shadows are soft.
     */
    boolean fancyGraphics = true;

    /**
     * Whether the screen waits for the vertical sync of the monitor.
     *
     * <p>With the vertical sync on, the game draws exactly as many
     * frames as the monitor shows, and sleeps in between. On a fast
     * machine that is a waste of nothing, but on a slow one it hides
     * the difference between the frames the game can draw and the
     * ones it cannot.
     */
    boolean vsync = true;

    /**
     * How far the view reaches, in chunks.
     *
     * <p>The value of zero means the one of the game. A smaller value
     * is the single biggest saving there is, and the one the player
     * notices the most.
     */
    int renderDistance = 0;

    /**
     * How far the world is simulated, in chunks.
     *
     * <p>The value of zero means the one of the game. The world is
     * simulated further than it is drawn, and the difference is pure
     * work for nothing.
     */
    int simulationDistance = 0;

    /**
     * The maximum number of frames the game waits for before it draws
     * a frame.
     *
     * <p>This is the frame cap, and it is the setting that decides how
     * many frames the game draws at all. A value of zero means the cap
     * of the game.
     */
    int frameLimit = 0;

    /**
     * Whether the chunks around the player are built in the
     * background.
     *
     * <p>When this is off, the game builds only the chunks it is about
     * to draw, and it builds them when it needs them. The chunks come
     * a little later, and the processor is free for the frames.
     */
    boolean backgroundChunks = true;

    /**
     * Whether the mod is allowed to do anything at all.
     */
    boolean enabled = true;

    /**
     * Build the settings of a preset.
     *
     * @param preset the name of the preset
     * @return the settings of the preset
     */
    public static OptimisationSettings of(String preset) {
        OptimisationSettings settings = new OptimisationSettings();
        settings.applyPreset(preset);
        return settings;
    }

    /**
     * The name of the preset.
     *
     * @return the name of the preset
     */
    public String getPreset() {
        return preset;
    }

    /**
     * Set the name of the preset, and everything it implies.
     *
     * @param preset the name of the preset
     */
    public void setPreset(String preset) {
        applyPreset(preset);
    }

    /**
     * Apply a preset.
     *
     * <p>A name that is not known is treated as the balanced one: the
     * mod must not leave the game in a half-changed state because of a
     * typo in a file.
     *
     * @param preset the name of the preset
     */
    public void applyPreset(String preset) {
        // A name that is not known is not kept: the settings of the
        // balanced one are used, and the name says so. A preset the
        // mod does not know is a typo, and the log has to show it.
        this.preset = BALANCED;

        if (FAST.equals(preset)) {
            this.preset = FAST;
            enabled = true;
            clouds = false;
            weather = false;
            particles = true;
            entityShadows = false;
            ambientOcclusion = false;
            fancyGraphics = false;
            vsync = false;
            renderDistance = 0;
            simulationDistance = 0;
            frameLimit = 0;
            backgroundChunks = false;
            return;
        }

        if (OFF.equals(preset)) {
            this.preset = OFF;
            enabled = false;
            clouds = true;
            weather = true;
            particles = true;
            entityShadows = true;
            ambientOcclusion = true;
            fancyGraphics = true;
            vsync = true;
            renderDistance = 0;
            simulationDistance = 0;
            frameLimit = 0;
            backgroundChunks = true;
            return;
        }

        // The balanced preset: the look of the game stays, and the
        // work that nothing can be seen of goes away.
        enabled = true;
        clouds = true;
        weather = true;
        particles = true;
        entityShadows = true;
        ambientOcclusion = true;
        fancyGraphics = true;
        vsync = true;
        renderDistance = 0;
        simulationDistance = 0;
        frameLimit = 0;
        backgroundChunks = true;
    }

    /**
     * Whether the mod is allowed to do anything.
     *
     * @return true when the mod works
     */
    public boolean isEnabled() {
        return enabled;
    }

    /**
     * Whether the clouds are drawn.
     *
     * @return true when the clouds are drawn
     */
    public boolean hasClouds() {
        return clouds;
    }

    /**
     * Whether the weather is drawn.
     *
     * @return true when the rain and the snow are drawn
     */
    public boolean hasWeather() {
        return weather;
    }

    /**
     * Whether the particles are drawn.
     *
     * @return true when the particles are drawn
     */
    public boolean hasParticles() {
        return particles;
    }

    /**
     * Whether the shadows of the entities are drawn.
     *
     * @return true when the shadows are drawn
     */
    public boolean hasEntityShadows() {
        return entityShadows;
    }

    /**
     * Whether the ambient occlusion is used.
     *
     * @return true when the ambient occlusion is used
     */
    public boolean hasAmbientOcclusion() {
        return ambientOcclusion;
    }

    /**
     * Whether the fancy graphics are used.
     *
     * @return true when the fancy graphics are used
     */
    public boolean hasFancyGraphics() {
        return fancyGraphics;
    }

    /**
     * Whether the screen waits for the vertical sync.
     *
     * @return true when the vertical sync is on
     */
    public boolean hasVsync() {
        return vsync;
    }

    /**
     * How far the view reaches, in chunks.
     *
     * @return the distance, or zero when the one of the game is used
     */
    public int getRenderDistance() {
        return renderDistance;
    }

    /**
     * How far the world is simulated, in chunks.
     *
     * @return the distance, or zero when the one of the game is used
     */
    public int getSimulationDistance() {
        return simulationDistance;
    }

    /**
     * The frame cap of the game.
     *
     * @return the cap, or zero when the one of the game is used
     */
    public int getFrameLimit() {
        return frameLimit;
    }

    /**
     * Whether the chunks are built in the background.
     *
     * @return true when the chunks are built in the background
     */
    public boolean hasBackgroundChunks() {
        return backgroundChunks;
    }

    /**
     * The names of the presets the mod knows.
     *
     * @return the names of the presets
     */
    public static String[] presets() {
        return new String[] {BALANCED, FAST, OFF};
    }
}
