export type TodoEvent =
  | { type: "Created"; id: string; title: string }
  | { type: "Completed"; completedAt: string }
  | { type: "TitleUpdated"; title: string };
