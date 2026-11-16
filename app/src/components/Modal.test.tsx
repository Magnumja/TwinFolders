import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Modal } from "./Modal";

describe("Modal", () => {
  it("keeps focus in an input when the parent re-renders with a new onClose", () => {
    const ui = (n: number) => (
      <Modal title="t" onClose={() => n}>
        <input aria-label="campo" />
      </Modal>
    );
    const { getByLabelText, rerender } = render(ui(1));
    const input = getByLabelText("campo") as HTMLInputElement;
    input.focus();
    expect(document.activeElement).toBe(input);
    rerender(ui(2)); // what the 3 s poll does
    expect(document.activeElement).toBe(input);
  });
});
