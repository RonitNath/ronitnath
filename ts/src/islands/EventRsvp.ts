import type { GuestView } from "../generated/GuestView";
import type { ScheduleItem } from "../generated/ScheduleItem";
import type { SegmentChoice } from "../generated/SegmentChoice";
import { fetchGuestView, postRsvp } from "../lib/events_api";

const PERSONAL_LINK_KEY = "gather-personal-link";
const RSVP_STATUSES = ["going", "maybe", "no"] as const;
const SEGMENT_STATUSES = ["in", "maybe", "out"] as const;

function button(label: string): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.textContent = label;
  return element;
}

/** Mount the RSVP form with the same state transitions and DOM/CSS contract
 * as the previous reactive island, using only browser DOM APIs. */
export function mountEventRsvp(mount: HTMLElement, endpoint: string): void {
  const loading = document.createElement("p");
  loading.className = "muted";
  loading.textContent = "Loading…";
  mount.replaceChildren(loading);

  void fetchGuestView(endpoint).then(
    (view) => renderForm(mount, endpoint, view),
    () => {
      const error = document.createElement("p");
      error.className = "rsvp-error";
      error.textContent = "Couldn't load the RSVP form — refresh to retry.";
      mount.replaceChildren(error);
    },
  );
}

function renderForm(mount: HTMLElement, endpoint: string, view: GuestView): void {
  const personalized = view.person !== null;
  const rsvpable = view.schedule.filter((item) => item.segment_key !== null);

  let status: string | null = view.person?.attendance?.status ?? null;
  let partySize = view.person?.attendance?.party_size ?? 1;
  let savedName: string | null = null;
  let personalUrl: string | null = null;
  const segments: Record<number, string> = {};
  for (const segment of view.person?.segments ?? []) {
    segments[segment.schedule_item_id] = segment.status;
  }

  const form = document.createElement("form");
  form.className = "rsvp-form";

  if (view.person) {
    const greeting = document.createElement("p");
    greeting.append("Hi ");
    const strong = document.createElement("strong");
    strong.textContent = view.person.name;
    greeting.append(
      strong,
      "! If any plans change, please update this, it'll be a huge help for my planning. Thanks!",
    );
    form.append(greeting);
  }

  let nameInput: HTMLInputElement | null = null;
  if (!personalized) {
    const nameLabel = document.createElement("label");
    nameLabel.append("Your name");
    nameInput = document.createElement("input");
    nameInput.type = "text";
    nameInput.required = true;
    nameInput.maxLength = 100;
    nameLabel.append(nameInput);
    form.append(nameLabel);
  }

  const choices = document.createElement("div");
  choices.className = "rsvp-choices";
  const statusButtons = new Map<string, HTMLButtonElement>();
  for (const choice of RSVP_STATUSES) {
    const choiceButton = button(
      choice === "going" ? "I'm in" : choice === "maybe" ? "Maybe" : "Can't make it",
    );
    choiceButton.classList.toggle("selected", status === choice);
    choiceButton.addEventListener("click", () => {
      status = choice;
      updateStatus();
    });
    statusButtons.set(choice, choiceButton);
    choices.append(choiceButton);
  }
  form.append(choices);

  let segmentGroup: HTMLDivElement | null = null;
  let partyLabel: HTMLLabelElement | null = null;

  const notesLabel = document.createElement("label");
  notesLabel.append("Notes");
  const noteInput = document.createElement("textarea");
  noteInput.placeholder = "Dietary needs, arrival time, or questions";
  noteInput.maxLength = 500;
  noteInput.value = view.person?.attendance?.note ?? "";
  notesLabel.append(noteInput);
  form.append(notesLabel);

  let errorElement: HTMLParagraphElement | null = null;

  const submit = document.createElement("button");
  submit.className = "rsvp-submit";
  submit.type = "submit";
  submit.textContent = "Save my status";
  form.append(submit);

  let savedElement: HTMLDivElement | null = null;

  function countFor(itemId: number): string {
    const count = view.segment_counts.find((item) => item.schedule_item_id === itemId);
    return count ? `${count.in_count} in` : "";
  }

  function createSegmentGroup(items: ScheduleItem[]): HTMLDivElement {
    const group = document.createElement("div");
    for (const item of items) {
      const row = document.createElement("div");
      row.className = "segment-row";

      const segmentName = document.createElement("span");
      segmentName.className = "segment-name";
      segmentName.textContent = `${item.time_label} · ${item.title}`;

      const segmentCount = document.createElement("span");
      segmentCount.className = "segment-count";
      segmentCount.textContent = countFor(item.id);

      row.append(segmentName, segmentCount);
      const itemButtons = new Map<string, HTMLButtonElement>();
      for (const choice of SEGMENT_STATUSES) {
        const choiceButton = button(choice);
        choiceButton.classList.toggle("selected", segments[item.id] === choice);
        choiceButton.addEventListener("click", () => {
          segments[item.id] = choice;
          for (const [value, element] of itemButtons) {
            element.classList.toggle("selected", value === choice);
          }
        });
        itemButtons.set(choice, choiceButton);
        row.append(choiceButton);
      }
      group.append(row);
    }
    return group;
  }

  function updateConditionalFields(): void {
    const showSegments = status !== "no" && rsvpable.length > 0;
    if (showSegments && !segmentGroup) {
      segmentGroup = createSegmentGroup(rsvpable);
      form.insertBefore(segmentGroup, partyLabel ?? notesLabel);
    } else if (!showSegments && segmentGroup) {
      segmentGroup.remove();
      segmentGroup = null;
    }

    if (status === "going" && !partyLabel) {
      partyLabel = document.createElement("label");
      partyLabel.append("Bringing anyone? Total heads including you:");
      const partyInput = document.createElement("input");
      partyInput.type = "number";
      partyInput.min = "1";
      partyInput.max = "10";
      partyInput.value = String(partySize);
      partyInput.addEventListener("input", () => {
        const nextPartySize = Number(partyInput.value) || 1;
        // Match signal semantics: writing the same value does not repaint the
        // input (so clearing an initial "1" stays clear while state remains 1).
        if (nextPartySize !== partySize) {
          partySize = nextPartySize;
          partyInput.value = String(partySize);
        }
      });
      partyLabel.append(partyInput);
      form.insertBefore(partyLabel, notesLabel);
    } else if (status !== "going" && partyLabel) {
      partyLabel.remove();
      partyLabel = null;
    }
  }

  function setError(message: string | null): void {
    errorElement?.remove();
    errorElement = null;
    if (message === null) return;

    errorElement = document.createElement("p");
    errorElement.className = "rsvp-error";
    errorElement.textContent = message;
    form.insertBefore(errorElement, submit);
  }

  function renderSaved(): void {
    savedElement?.remove();
    savedElement = null;
    if (!savedName) return;

    savedElement = document.createElement("div");
    savedElement.className = "rsvp-saved";
    const confirmation = document.createElement("p");
    confirmation.textContent = `Saved — thanks, ${savedName}!${
      status === "going" ? " See you there." : ""
    }`;
    savedElement.append(confirmation);

    if (personalUrl) {
      const linkMessage = document.createElement("p");
      linkMessage.append(
        "Save this personal link — it's your page for updating plans later:",
        document.createElement("br"),
      );
      const link = document.createElement("a");
      link.className = "rsvp-personal-link";
      link.href = personalUrl;
      link.textContent = personalUrl;
      linkMessage.append(link);
      savedElement.append(linkMessage);
    }
    form.append(savedElement);
  }

  function updateStatus(): void {
    for (const [value, element] of statusButtons) {
      element.classList.toggle("selected", status === value);
    }
    updateConditionalFields();
    renderSaved();
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (status === null) {
      setError("Pick going, maybe, or can't make it first.");
      return;
    }

    setError(null);
    submit.disabled = true;
    submit.textContent = "Saving…";
    try {
      const chosen: SegmentChoice[] = Object.entries(segments).map(([id, choice]) => ({
        schedule_item_id: Number(id),
        status: choice,
      }));
      const result = await postRsvp(endpoint, {
        name: personalized ? null : (nameInput?.value ?? ""),
        status,
        party_size: partySize,
        note: noteInput.value,
        segments: chosen,
      });
      savedName = result.person_name;
      if (result.personal_url) {
        personalUrl = result.personal_url;
        try {
          localStorage.setItem(PERSONAL_LINK_KEY, result.personal_url);
        } catch {
          // Private mode can deny storage; the link remains visible on screen.
        }
      }
      renderSaved();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Something went wrong.");
    } finally {
      submit.disabled = false;
      submit.textContent = savedName ? "Update my status" : "Save my status";
    }
  });

  updateConditionalFields();
  mount.replaceChildren(form);
}
