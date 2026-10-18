/**
 * The live menu model: the workbench commands and the plugin commands the
 * kernel lists, built into menus (`buildMenus`) for the title bar and for the
 * macOS application menu alike.
 */

import { buildMenus, type Menu } from "$lib/commands/menu";
import { workbenchCommands } from "$lib/commands/workbench";
import { kernel, reportError, type KernelCommand } from "$lib/kernel";
import { runKernelCommand } from "$lib/state/actions";
import { context } from "$lib/state/context.svelte";
import { palette } from "$lib/state/palette.svelte";
import { detectPlatform } from "$lib/util/keyboard";

class MenuModelState {
  /** Plugin commands, for `[[contributes.menus]]` placements. */
  kernelCommands = $state.raw<readonly KernelCommand[]>([]);
  #loading = false;

  /** Ask the kernel for its commands once. */
  load(): void {
    if (this.#loading) return;
    this.#loading = true;
    kernel
      .commands()
      .then((commands) => (this.kernelCommands = commands))
      .catch((error: unknown) =>
        reportError("plugin menu items could not be listed", error),
      );
  }

  /** The menus as they are now. Reactive: read inside `$derived` to follow changes. */
  build(): Menu[] {
    return buildMenus({
      commands: workbenchCommands((mode) => palette.show(mode)),
      kernel: this.kernelCommands,
      runKernel: (id) => {
        runKernelCommand(id).catch((error: unknown) =>
          reportError(`menu command ${id} failed`, error),
        );
      },
      applies: (when) => context.when(when),
      platform: detectPlatform(),
    });
  }
}

export const menuModel = new MenuModelState();
