/**
 * The setup of the tests.
 *
 * The matchers of jest-dom are added here, so that a test can say
 * `expect(element).toBeInTheDocument()` instead of a long chain of
 * property checks.
 */

import "@testing-library/jest-dom/vitest";

// jsdom does not implement the dialog methods the update notice calls.
if (!HTMLDialogElement.prototype.showModal) {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.open = false;
  };
}
