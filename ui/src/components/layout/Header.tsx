import { 
  Badge, 
  Tooltip, 
  TooltipContent, 
  TooltipTrigger, 
  TooltipProvider,
} from '@kostovster/ui';

interface HeaderProps {
  systemStatus: 'healthy' | 'warning' | 'error' | 'maintenance';
  statusMessage: string;
}

const statusVariantMap = {
  healthy: 'default' as const,
  warning: 'secondary' as const,
  error: 'destructive' as const,
  maintenance: 'outline' as const,
};

export function Header({ systemStatus, statusMessage }: HeaderProps) {
  return (
    <TooltipProvider>
      <header className="border-b bg-card">
        <div className="max-w-7xl mx-auto px-8 py-6 flex justify-between items-center">
          <div className="flex items-center gap-3">
            <img 
              src="https://nolus.io/favicon/favicon.svg" 
              alt="Nolus" 
              className="h-6 w-6"
            />
            <h1 className="text-xl font-semibold text-foreground">
              Nodes
            </h1>
          </div>
          
          <Tooltip>
            <TooltipTrigger asChild>
              <div>
                <Badge variant={statusVariantMap[systemStatus]} className="cursor-help">
                  {statusMessage}
                </Badge>
              </div>
            </TooltipTrigger>
            <TooltipContent>
              <p>Current system status</p>
            </TooltipContent>
          </Tooltip>
        </div>
      </header>
    </TooltipProvider>
  );
}
