package dev.minlaucher.mod;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * The file the settings of the mod live in.
 *
 * <p>The launcher writes the file before it starts the game, and the
 * mod reads it when it starts. That way the settings of the mod are
 * the settings of the launcher, and there is one place where the
 * player changes them.
 *
 * <p>The file is written with Gson, which the game already has: a mod
 * that brings a parser of its own makes the class path longer for
 * everybody, and the game has one that works.
 */
public final class SettingsStore {

    /**
     * The name of the file, in the directory of the game.
     */
    public static final String FILE_NAME = "minlaucher-optimisations.json";

    /**
     * The directory the settings are read from.
     *
     * <p>It is the working directory of the game, which the launcher
     * sets to the directory of the instance.
     */
    private final Path directory;

    /**
     * Build a store for a directory.
     *
     * @param directory the directory of the game
     */
    public SettingsStore(Path directory) {
        this.directory = directory;
    }

    /**
     * The file the settings live in.
     *
     * @return the file
     */
    public Path file() {
        return directory.resolve(FILE_NAME);
    }

    /**
     * Read the settings.
     *
     * <p>A file that is missing, or that cannot be read, gives the
     * balanced preset: the game must start even when the launcher
     * could not write the settings.
     *
     * @return the settings
     */
    public OptimisationSettings load() {
        Path file = file();
        if (!Files.isRegularFile(file)) {
            return OptimisationSettings.of(OptimisationSettings.BALANCED);
        }

        try {
            String text = new String(Files.readAllBytes(file), StandardCharsets.UTF_8);
            return parse(text);
        } catch (IOException | RuntimeException failure) {
            return OptimisationSettings.of(OptimisationSettings.BALANCED);
        }
    }

    /**
     * Read the settings out of a document.
     *
     * <p>The document is a flat object of strings and numbers, in the
     * form the launcher writes. A field that is not there keeps the
     * value of the preset, and a value that makes no sense is
     * ignored: the game must start whatever the file says.
     *
     * @param text the document
     * @return the settings
     */
    public static OptimisationSettings parse(String text) {
        OptimisationSettings settings = new OptimisationSettings();

        JsonObject object;
        try {
            object = JsonParser.parseString(text).getAsJsonObject();
        } catch (RuntimeException broken) {
            return OptimisationSettings.of(OptimisationSettings.BALANCED);
        }

        if (object.has("preset") && object.get("preset").isJsonPrimitive()) {
            settings.setPreset(object.get("preset").getAsString());
        }

        settings.enabled = readBoolean(object, "enabled", settings.isEnabled());
        settings.clouds = readBoolean(object, "clouds", settings.hasClouds());
        settings.weather = readBoolean(object, "weather", settings.hasWeather());
        settings.particles = readBoolean(object, "particles", settings.hasParticles());
        settings.entityShadows = readBoolean(object, "entity_shadows", settings.hasEntityShadows());
        settings.ambientOcclusion =
                readBoolean(object, "ambient_occlusion", settings.hasAmbientOcclusion());
        settings.fancyGraphics = readBoolean(object, "fancy_graphics", settings.hasFancyGraphics());
        settings.vsync = readBoolean(object, "vsync", settings.hasVsync());
        settings.backgroundChunks =
                readBoolean(object, "background_chunks", settings.hasBackgroundChunks());
        settings.renderDistance = readDistance(object, "render_distance", settings.getRenderDistance());
        settings.simulationDistance =
                readDistance(object, "simulation_distance", settings.getSimulationDistance());
        settings.frameLimit = readDistance(object, "frame_limit", settings.getFrameLimit());

        return settings;
    }

    /**
     * Read a switch out of a document.
     *
     * @param object the document
     * @param name the name of the field
     * @param fallback the value to use when the field is not there
     * @return the value of the field
     */
    private static boolean readBoolean(JsonObject object, String name, boolean fallback) {
        if (!object.has(name) || !object.get(name).isJsonPrimitive()) {
            return fallback;
        }
        try {
            return object.get(name).getAsBoolean();
        } catch (RuntimeException broken) {
            return fallback;
        }
    }

    /**
     * Read a number out of a document.
     *
     * <p>A number that is negative is not a distance and is not a cap,
     * so it is ignored.
     *
     * @param object the document
     * @param name the name of the field
     * @param fallback the value to use when the field is not there
     * @return the value of the field
     */
    private static int readDistance(JsonObject object, String name, int fallback) {
        if (!object.has(name) || !object.get(name).isJsonPrimitive()) {
            return fallback;
        }
        try {
            int value = object.get(name).getAsInt();
            return value >= 0 ? value : fallback;
        } catch (RuntimeException broken) {
            return fallback;
        }
    }

    /**
     * Write the settings.
     *
     * <p>The file is written next to the target and then moved over
     * it, so that the game never reads half a document.
     *
     * @param settings the settings
     * @throws IOException when the file cannot be written
     */
    public void save(OptimisationSettings settings) throws IOException {
        Files.createDirectories(directory);
        Path file = file();
        Path temporary = file.resolveSibling(FILE_NAME + ".tmp");

        Files.write(temporary, toJson(settings).getBytes(StandardCharsets.UTF_8));
        try {
            Files.move(
                    temporary,
                    file,
                    StandardCopyOption.REPLACE_EXISTING,
                    StandardCopyOption.ATOMIC_MOVE);
        } catch (java.nio.file.AtomicMoveNotSupportedException ignored) {
            Files.move(temporary, file, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    /**
     * The settings as a document.
     *
     * @param settings the settings
     * @return the document
     */
    public static String toJson(OptimisationSettings settings) {
        JsonObject object = new JsonObject();
        object.addProperty("preset", settings.getPreset());
        object.addProperty("enabled", settings.isEnabled());
        object.addProperty("clouds", settings.hasClouds());
        object.addProperty("weather", settings.hasWeather());
        object.addProperty("particles", settings.hasParticles());
        object.addProperty("entity_shadows", settings.hasEntityShadows());
        object.addProperty("ambient_occlusion", settings.hasAmbientOcclusion());
        object.addProperty("fancy_graphics", settings.hasFancyGraphics());
        object.addProperty("vsync", settings.hasVsync());
        object.addProperty("background_chunks", settings.hasBackgroundChunks());
        object.addProperty("render_distance", settings.getRenderDistance());
        object.addProperty("simulation_distance", settings.getSimulationDistance());
        object.addProperty("frame_limit", settings.getFrameLimit());

        Gson gson = new GsonBuilder().setPrettyPrinting().create();
        return gson.toJson(object) + "\n";
    }
}
