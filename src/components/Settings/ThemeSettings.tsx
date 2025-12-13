import { Paintbrush } from "lucide-react";
import { useState } from "react";
import { Button } from "../ui/button";
import { ThemeDesigner } from "./ThemeDesigner";
import { ThemePicker } from "./ThemePicker";

export function ThemeSettings() {
  const [designerOpen, setDesignerOpen] = useState(false);
  const [editThemeId, setEditThemeId] = useState<string | null>(null);

  const handleCreateTheme = () => {
    setEditThemeId(null);
    setDesignerOpen(true);
  };

  const handleEditTheme = (themeId: string) => {
    setEditThemeId(themeId);
    setDesignerOpen(true);
  };

  return (
    <div className="space-y-0">
      {/* Theme Designer Button */}
      <div className="space-y-2 mb-4">
        <h3 className="text-sm font-medium text-foreground">Theme Designer</h3>
        <Button onClick={handleCreateTheme} className="w-full" variant="outline">
          <Paintbrush className="w-4 h-4 mr-2" />
          Create New Theme
        </Button>
        <p className="text-xs text-muted-foreground">
          Design custom themes with real-time preview
        </p>
      </div>

      {/* Theme Picker */}
      <div className="space-y-2">
        <h3 className="text-sm font-medium text-foreground mb-4">Theme Selection</h3>
        <ThemePicker onEditTheme={handleEditTheme} />
      </div>

      {/* Info */}
      <div className="mt-6 p-4 rounded-md bg-muted/50">
        <p className="text-sm text-muted-foreground">
          Themes control the appearance of the entire application, including colors, typography,
          and background images. You can import custom themes or use the built-in options.
        </p>
      </div>

      {/* Theme Designer Dialog */}
      <ThemeDesigner
        open={designerOpen}
        onOpenChange={setDesignerOpen}
        editThemeId={editThemeId}
      />
    </div>
  );
}
