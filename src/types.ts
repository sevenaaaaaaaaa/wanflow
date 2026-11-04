export interface Cmd {
  id: string;
  label: string;
  group: string;
  kbd?: string;
  hint?: string;
  run: () => void;
}
