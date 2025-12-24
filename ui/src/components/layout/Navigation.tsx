import { NavLink } from 'react-router-dom';
import {
  Badge,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
  Separator,
  cn,
} from '@kostovster/ui';
import { LayoutDashboard, Boxes, Server } from 'lucide-react';

interface NavigationProps {
  systemStatus: 'healthy' | 'degraded' | 'critical';
  statusMessage?: string;
}

const navItems = [
  { to: '/', label: 'Dashboard', icon: LayoutDashboard },
  { to: '/nodes', label: 'Nodes', icon: Boxes },
  { to: '/services', label: 'Services', icon: Server },
];

const statusVariantMap: Record<string, 'default' | 'secondary' | 'destructive'> = {
  healthy: 'default',
  degraded: 'secondary',
  critical: 'destructive',
};

export function Navigation({ systemStatus, statusMessage }: NavigationProps) {
  return (
    <TooltipProvider>
      <header className="sticky top-0 z-50 border-b bg-card/95 backdrop-blur supports-[backdrop-filter]:bg-card/60">
        <div className="max-w-7xl mx-auto px-8">
          <div className="flex h-16 items-center justify-between">
            {/* Logo and Navigation */}
            <div className="flex items-center gap-6">
              <NavLink to="/" className="flex items-center hover:opacity-80 transition-opacity">
                <img
                  src="https://nolus.io/favicon/favicon.svg"
                  alt="Nolus"
                  className="h-7 w-7"
                />
              </NavLink>

              <Separator orientation="vertical" className="h-6" />

              {/* Navigation Links */}
              <nav className="flex items-center gap-1">
                {navItems.map(({ to, label, icon: Icon }) => (
                  <NavLink
                    key={to}
                    to={to}
                    className={({ isActive }) =>
                      cn(
                        'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium transition-colors',
                        'hover:text-foreground',
                        isActive
                          ? 'bg-muted text-foreground'
                          : 'text-muted-foreground'
                      )
                    }
                  >
                    <Icon className="h-4 w-4" />
                    {label}
                  </NavLink>
                ))}
              </nav>
            </div>

            {/* Status Badge */}
            <div className="flex items-center gap-4">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge variant={statusVariantMap[systemStatus]} className="cursor-help capitalize">
                    {systemStatus}
                  </Badge>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{statusMessage || `System is ${systemStatus}`}</p>
                </TooltipContent>
              </Tooltip>
            </div>
          </div>
        </div>
      </header>
    </TooltipProvider>
  );
}
