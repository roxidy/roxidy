import {
  AlertCircle,
  Bell,
  Check,
  ChevronRight,
  Edit,
  Info,
  Loader2,
  Mail,
  Plus,
  Search,
  Settings,
  Terminal,
  Trash,
  User,
  X,
} from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
// Import all shadcn components
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { Toggle } from "@/components/ui/toggle";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

interface SectionProps {
  title: string;
  children: React.ReactNode;
}

function Section({ title, children }: SectionProps) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <Collapsible
      open={isOpen}
      onOpenChange={setIsOpen}
      className="border border-[#27293d] rounded-lg mb-4"
    >
      <CollapsibleTrigger className="flex items-center justify-between w-full p-4 hover:bg-[#1f2335] transition-colors">
        <h2 className="text-lg font-semibold text-[#c0caf5]">{title}</h2>
        <ChevronRight
          className={`w-5 h-5 text-[#565f89] transition-transform ${isOpen ? "rotate-90" : ""}`}
        />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="p-4 pt-0 space-y-4">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}

export function ComponentTestbed() {
  const [switchValue, setSwitchValue] = useState(false);
  const [toggleValue, setToggleValue] = useState(false);

  return (
    <TooltipProvider>
      <div className="min-h-screen bg-[#1a1b26] text-[#c0caf5]">
        {/* Header */}
        <div className="sticky top-0 z-10 bg-[#16161e] border-b border-[#27293d] px-6 py-4">
          <h1 className="text-2xl font-bold">Component Testbed</h1>
          <p className="text-sm text-[#565f89] mt-1">All installed shadcn/ui components</p>
        </div>

        <ScrollArea className="h-[calc(100vh-80px)]">
          <div className="max-w-4xl mx-auto p-6 space-y-6">
            {/* Button */}
            <Section title="Button">
              <div className="flex flex-wrap gap-3">
                <Button>Default</Button>
                <Button variant="secondary">Secondary</Button>
                <Button variant="destructive">Destructive</Button>
                <Button variant="outline">Outline</Button>
                <Button variant="ghost">Ghost</Button>
                <Button variant="link">Link</Button>
              </div>
              <div className="flex flex-wrap gap-3">
                <Button size="sm">Small</Button>
                <Button size="default">Default</Button>
                <Button size="lg">Large</Button>
                <Button size="icon">
                  <Plus className="w-4 h-4" />
                </Button>
              </div>
              <div className="flex flex-wrap gap-3">
                <Button disabled>Disabled</Button>
                <Button disabled>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Loading
                </Button>
              </div>
            </Section>

            {/* Input & Textarea */}
            <Section title="Input & Textarea">
              <div className="space-y-3">
                <Input placeholder="Default input" />
                <Input placeholder="Disabled input" disabled />
                <Input type="password" placeholder="Password input" />
                <Textarea placeholder="Textarea for longer content..." />
              </div>
            </Section>

            {/* Badge */}
            <Section title="Badge">
              <div className="flex flex-wrap gap-3">
                <Badge>Default</Badge>
                <Badge variant="secondary">Secondary</Badge>
                <Badge variant="destructive">Destructive</Badge>
                <Badge variant="outline">Outline</Badge>
              </div>
              <div className="flex flex-wrap gap-3">
                <Badge>
                  <Check className="w-3 h-3 mr-1" />
                  Success
                </Badge>
                <Badge variant="destructive">
                  <X className="w-3 h-3 mr-1" />
                  Error
                </Badge>
                <Badge variant="secondary">
                  <AlertCircle className="w-3 h-3 mr-1" />
                  Warning
                </Badge>
                <Badge variant="outline">
                  <Info className="w-3 h-3 mr-1" />
                  Info
                </Badge>
              </div>
            </Section>

            {/* Card */}
            <Section title="Card">
              <Card>
                <CardHeader>
                  <CardTitle>Card Title</CardTitle>
                  <CardDescription>Card description goes here</CardDescription>
                </CardHeader>
                <CardContent>
                  <p>This is the card content area. You can put any content here.</p>
                </CardContent>
                <CardFooter className="gap-2">
                  <Button variant="outline">Cancel</Button>
                  <Button>Save</Button>
                </CardFooter>
              </Card>
            </Section>

            {/* Collapsible */}
            <Section title="Collapsible">
              <Collapsible className="border border-[#27293d] rounded-lg">
                <CollapsibleTrigger className="flex items-center justify-between w-full p-3 hover:bg-[#1f2335]">
                  <span>Click to expand</span>
                  <ChevronRight className="w-4 h-4" />
                </CollapsibleTrigger>
                <CollapsibleContent className="p-3 pt-0 text-sm text-[#a9b1d6]">
                  This content is hidden by default and shown when the trigger is clicked.
                </CollapsibleContent>
              </Collapsible>
            </Section>

            {/* Dialog */}
            <Section title="Dialog">
              <Dialog>
                <DialogTrigger asChild>
                  <Button>Open Dialog</Button>
                </DialogTrigger>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Dialog Title</DialogTitle>
                    <DialogDescription>
                      This is a dialog description. It provides additional context.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="py-4">
                    <p>Dialog content goes here.</p>
                  </div>
                  <DialogFooter>
                    <Button variant="outline">Cancel</Button>
                    <Button>Confirm</Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            </Section>

            {/* Dropdown Menu */}
            <Section title="Dropdown Menu">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline">Open Menu</Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent>
                  <DropdownMenuLabel>My Account</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem>
                    <User className="w-4 h-4 mr-2" />
                    Profile
                  </DropdownMenuItem>
                  <DropdownMenuItem>
                    <Settings className="w-4 h-4 mr-2" />
                    Settings
                  </DropdownMenuItem>
                  <DropdownMenuItem>
                    <Mail className="w-4 h-4 mr-2" />
                    Messages
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem className="text-[#f7768e]">
                    <Trash className="w-4 h-4 mr-2" />
                    Delete
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </Section>

            {/* Context Menu */}
            <Section title="Context Menu">
              <ContextMenu>
                <ContextMenuTrigger className="flex h-32 w-full items-center justify-center rounded-md border border-dashed border-[#27293d] text-sm">
                  Right-click here
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuItem>
                    <Edit className="w-4 h-4 mr-2" />
                    Edit
                  </ContextMenuItem>
                  <ContextMenuItem>
                    <Plus className="w-4 h-4 mr-2" />
                    Duplicate
                  </ContextMenuItem>
                  <ContextMenuItem className="text-[#f7768e]">
                    <Trash className="w-4 h-4 mr-2" />
                    Delete
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            </Section>

            {/* Popover */}
            <Section title="Popover">
              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline">Open Popover</Button>
                </PopoverTrigger>
                <PopoverContent>
                  <div className="space-y-2">
                    <h4 className="font-medium">Popover Title</h4>
                    <p className="text-sm text-[#a9b1d6]">This is a popover with some content.</p>
                  </div>
                </PopoverContent>
              </Popover>
            </Section>

            {/* Tooltip */}
            <Section title="Tooltip">
              <div className="flex gap-3">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon">
                      <Bell className="w-4 h-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Notifications</p>
                  </TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon">
                      <Settings className="w-4 h-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom">
                    <p>Settings</p>
                  </TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon">
                      <Search className="w-4 h-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    <p>Search (⌘K)</p>
                  </TooltipContent>
                </Tooltip>
              </div>
            </Section>

            {/* Tabs */}
            <Section title="Tabs">
              <Tabs defaultValue="tab1">
                <TabsList>
                  <TabsTrigger value="tab1">Account</TabsTrigger>
                  <TabsTrigger value="tab2">Password</TabsTrigger>
                  <TabsTrigger value="tab3">Settings</TabsTrigger>
                </TabsList>
                <TabsContent value="tab1" className="p-4 border border-[#27293d] rounded-b-lg">
                  <p>Account settings content</p>
                </TabsContent>
                <TabsContent value="tab2" className="p-4 border border-[#27293d] rounded-b-lg">
                  <p>Password settings content</p>
                </TabsContent>
                <TabsContent value="tab3" className="p-4 border border-[#27293d] rounded-b-lg">
                  <p>General settings content</p>
                </TabsContent>
              </Tabs>
            </Section>

            {/* Toggle */}
            <Section title="Toggle">
              <div className="flex gap-3">
                <Toggle pressed={toggleValue} onPressedChange={setToggleValue}>
                  <Terminal className="w-4 h-4 mr-2" />
                  Toggle {toggleValue ? "On" : "Off"}
                </Toggle>
                <Toggle variant="outline">
                  <Edit className="w-4 h-4" />
                </Toggle>
                <Toggle disabled>Disabled</Toggle>
              </div>
            </Section>

            {/* Switch */}
            <Section title="Switch">
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                  <Switch checked={switchValue} onCheckedChange={setSwitchValue} />
                  <span className="text-sm">{switchValue ? "Enabled" : "Disabled"}</span>
                </div>
                <div className="flex items-center gap-2">
                  <Switch disabled />
                  <span className="text-sm text-[#565f89]">Disabled</span>
                </div>
              </div>
            </Section>

            {/* Separator */}
            <Section title="Separator">
              <div className="space-y-4">
                <div>
                  <p className="text-sm">Content above</p>
                  <Separator className="my-4" />
                  <p className="text-sm">Content below</p>
                </div>
                <div className="flex items-center gap-4 h-8">
                  <span className="text-sm">Item 1</span>
                  <Separator orientation="vertical" />
                  <span className="text-sm">Item 2</span>
                  <Separator orientation="vertical" />
                  <span className="text-sm">Item 3</span>
                </div>
              </div>
            </Section>

            {/* Skeleton */}
            <Section title="Skeleton">
              <div className="space-y-4">
                <div className="flex items-center gap-4">
                  <Skeleton className="h-12 w-12 rounded-full" />
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-[250px]" />
                    <Skeleton className="h-4 w-[200px]" />
                  </div>
                </div>
                <Skeleton className="h-[125px] w-full rounded-xl" />
              </div>
            </Section>

            {/* Sheet */}
            <Section title="Sheet">
              <div className="flex gap-3">
                <Sheet>
                  <SheetTrigger asChild>
                    <Button variant="outline">Open Left Sheet</Button>
                  </SheetTrigger>
                  <SheetContent side="left">
                    <SheetHeader>
                      <SheetTitle>Left Sheet</SheetTitle>
                      <SheetDescription>This sheet slides in from the left side.</SheetDescription>
                    </SheetHeader>
                    <div className="py-4">
                      <p>Sheet content goes here.</p>
                    </div>
                  </SheetContent>
                </Sheet>
                <Sheet>
                  <SheetTrigger asChild>
                    <Button variant="outline">Open Right Sheet</Button>
                  </SheetTrigger>
                  <SheetContent side="right">
                    <SheetHeader>
                      <SheetTitle>Right Sheet</SheetTitle>
                      <SheetDescription>This sheet slides in from the right side.</SheetDescription>
                    </SheetHeader>
                    <div className="py-4">
                      <p>Sheet content goes here.</p>
                    </div>
                  </SheetContent>
                </Sheet>
              </div>
            </Section>

            {/* ScrollArea */}
            <Section title="ScrollArea">
              <ScrollArea className="h-48 w-full rounded-md border border-[#27293d] p-4">
                <div className="space-y-4">
                  {Array.from({ length: 20 }).map((_, i) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: static demo content
                    <div key={i} className="text-sm">
                      Scrollable item {i + 1}
                    </div>
                  ))}
                </div>
              </ScrollArea>
            </Section>
          </div>
        </ScrollArea>
      </div>
    </TooltipProvider>
  );
}
