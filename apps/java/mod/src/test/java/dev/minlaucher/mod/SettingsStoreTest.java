package dev.minlaucher.mod;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * The tests of the file the settings of the mod live in.
 */
class SettingsStoreTest {

    @Test
    void a_file_that_is_not_there_gives_the_balanced_preset(@TempDir Path directory) {
        OptimisationSettings settings = new SettingsStore(directory).load();

        assertEquals(OptimisationSettings.BALANCED, settings.getPreset());
        assertTrue(settings.isEnabled());
    }

    @Test
    void the_preset_of_the_file_is_used() {
        OptimisationSettings settings =
                SettingsStore.parse("{\"preset\": \"fast\"}");

        assertEquals(OptimisationSettings.FAST, settings.getPreset());
        assertFalse(settings.hasClouds());
    }

    @Test
    void the_file_can_set_the_values_one_by_one() {
        OptimisationSettings settings = SettingsStore.parse(
                "{\"preset\": \"balanced\", \"clouds\": true, \"render_distance\": 6, \"frame_limit\": 120}");

        assertTrue(settings.hasClouds());
        assertEquals(6, settings.getRenderDistance());
        assertEquals(120, settings.getFrameLimit());
    }

    @Test
    void a_value_that_is_not_there_keeps_the_one_of_the_preset() {
        OptimisationSettings settings =
                SettingsStore.parse("{\"preset\": \"fast\", \"render_distance\": 8}");

        assertEquals(8, settings.getRenderDistance());
        // The rest of the fast preset is still there.
        assertFalse(settings.hasFancyGraphics());
    }

    @Test
    void a_document_that_is_broken_gives_the_balanced_preset() {
        OptimisationSettings settings = SettingsStore.parse("{ this is not json");

        assertEquals(OptimisationSettings.BALANCED, settings.getPreset());
        assertTrue(settings.isEnabled());
    }

    @Test
    void a_negative_distance_is_ignored() {
        // A distance that is negative is not a distance, and the game
        // must start whatever the file says.
        OptimisationSettings settings =
                SettingsStore.parse("{\"preset\": \"balanced\", \"render_distance\": -4}");

        assertEquals(0, settings.getRenderDistance());
    }

    @Test
    void the_settings_are_written_and_read_back(@TempDir Path directory) throws IOException {
        SettingsStore store = new SettingsStore(directory);
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.FAST);
        settings.renderDistance = 4;

        store.save(settings);
        OptimisationSettings read = store.load();

        assertEquals(OptimisationSettings.FAST, read.getPreset());
        assertEquals(4, read.getRenderDistance());
        assertFalse(read.hasClouds());
    }

    @Test
    void the_file_is_written_where_the_game_looks_for_it(@TempDir Path directory) throws IOException {
        SettingsStore store = new SettingsStore(directory);

        store.save(OptimisationSettings.of(OptimisationSettings.BALANCED));

        Path expected = directory.resolve(SettingsStore.FILE_NAME);
        assertTrue(Files.isRegularFile(expected));
    }

    @Test
    void the_document_holds_every_setting(@TempDir Path directory) throws IOException {
        SettingsStore store = new SettingsStore(directory);

        store.save(OptimisationSettings.of(OptimisationSettings.BALANCED));
        String text = new String(
                Files.readAllBytes(store.file()), StandardCharsets.UTF_8);

        for (String name : new String[] {
            "preset", "enabled", "clouds", "weather", "particles", "entity_shadows",
            "ambient_occlusion", "fancy_graphics", "vsync", "background_chunks",
            "render_distance", "simulation_distance", "frame_limit"
        }) {
            assertTrue(text.contains("\"" + name + "\""), "the document has no " + name);
        }
    }
}
