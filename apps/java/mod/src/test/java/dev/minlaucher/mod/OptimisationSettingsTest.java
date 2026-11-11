package dev.minlaucher.mod;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * The tests of the settings of the mod.
 */
class OptimisationSettingsTest {

    @Test
    void the_balanced_preset_keeps_the_look_of_the_game() {
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.BALANCED);

        assertTrue(settings.isEnabled());
        assertTrue(settings.hasFancyGraphics());
        assertTrue(settings.hasEntityShadows());
        assertTrue(settings.hasAmbientOcclusion());
        assertTrue(settings.hasParticles());
        assertTrue(settings.hasWeather());
        assertTrue(settings.hasVsync());
    }

    @Test
    void the_balanced_preset_leaves_out_the_clouds() {
        // The clouds are a whole extra pass over the sky, every
        // frame, and they are the one thing nobody would miss.
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.BALANCED);

        assertFalse(settings.hasClouds());
    }

    @Test
    void the_fast_preset_turns_off_the_expensive_effects() {
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.FAST);

        assertTrue(settings.isEnabled());
        assertFalse(settings.hasClouds());
        assertFalse(settings.hasWeather());
        assertFalse(settings.hasFancyGraphics());
        assertFalse(settings.hasEntityShadows());
        assertFalse(settings.hasAmbientOcclusion());
        assertFalse(settings.hasVsync());
        assertFalse(settings.hasBackgroundChunks());
    }

    @Test
    void the_fast_preset_keeps_the_particles() {
        // The particles are cheap next to the clouds, and they are
        // what the player looks at the most.
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.FAST);

        assertTrue(settings.hasParticles());
    }

    @Test
    void the_off_preset_leaves_the_game_alone() {
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.OFF);

        assertFalse(settings.isEnabled());
        assertTrue(settings.hasClouds());
        assertTrue(settings.hasFancyGraphics());
        assertTrue(settings.hasEntityShadows());
        assertTrue(settings.hasAmbientOcclusion());
        assertTrue(settings.hasParticles());
        assertTrue(settings.hasWeather());
        assertTrue(settings.hasVsync());
        assertTrue(settings.hasBackgroundChunks());
    }

    @Test
    void a_preset_that_is_not_known_is_the_balanced_one() {
        // A typo in a file must not leave the game half-changed.
        OptimisationSettings settings = OptimisationSettings.of("turbo");

        assertEquals(OptimisationSettings.BALANCED, settings.getPreset());
        assertTrue(settings.isEnabled());
    }

    @Test
    void the_preset_starts_from_the_balanced_one() {
        OptimisationSettings settings = new OptimisationSettings();

        assertEquals(OptimisationSettings.BALANCED, settings.getPreset());
        assertTrue(settings.isEnabled());
    }

    @Test
    void a_distance_of_zero_means_the_one_of_the_game() {
        OptimisationSettings settings = OptimisationSettings.of(OptimisationSettings.BALANCED);

        assertEquals(0, settings.getRenderDistance());
        assertEquals(0, settings.getSimulationDistance());
        assertEquals(0, settings.getFrameLimit());
    }

    @Test
    void the_names_of_the_presets_are_known() {
        assertEquals(3, OptimisationSettings.presets().length);
    }
}
