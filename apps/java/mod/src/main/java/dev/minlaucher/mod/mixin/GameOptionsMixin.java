package dev.minlaucher.mod.mixin;

import dev.minlaucher.mod.MinLauncherMod;
import dev.minlaucher.mod.OptionApplier;
import net.minecraft.client.option.GameOptions;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Puts the settings of the launcher into the options of the game.
 *
 * <p>The game reads its options from its own file when it starts, and
 * every time the player presses the keys that reload them. The mixin
 * is on the reading, at the end of it: that way the mod always has
 * the last word, and it never has to know when exactly the game
 * reads them.
 *
 * <p>There is one method here on purpose. The more of the game a mod
 * reaches into, the more often it breaks when the game changes.
 */
@Mixin(GameOptions.class)
public abstract class GameOptionsMixin {

    /**
     * Change the options of the game, after it has read them.
     *
     * @param info the information of the injection
     */
    @Inject(method = "load", at = @At("TAIL"))
    private void minlaucher$applyOptions(CallbackInfo info) {
        OptionApplier.apply((GameOptions) (Object) this, MinLauncherMod.settings());
    }
}
