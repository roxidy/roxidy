import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../ui/dialog";
import { ThemePicker } from "./ThemePicker";

export function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  // Close on Cmd+, handled in App; this component is just the dialog
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>
        <div className="space-y-6">
          <div>
            <h3 className="text-sm font-medium mb-2">Theme</h3>
            <ThemePicker />
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
