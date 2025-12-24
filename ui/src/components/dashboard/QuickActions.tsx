import { useNavigate } from 'react-router-dom';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Button,
} from '@kostovster/ui';
import { Zap, Boxes, Server, ExternalLink } from 'lucide-react';

export function QuickActions() {
  const navigate = useNavigate();

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="flex items-center gap-2 text-base">
          <Zap className="h-5 w-5" />
          Quick Actions
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-3">
          <Button
            variant="outline"
            className="h-auto py-4 flex flex-col items-center gap-2"
            onClick={() => navigate('/nodes')}
          >
            <Boxes className="h-5 w-5" />
            <span className="text-xs">Manage Nodes</span>
          </Button>
          <Button
            variant="outline"
            className="h-auto py-4 flex flex-col items-center gap-2"
            onClick={() => navigate('/services')}
          >
            <Server className="h-5 w-5" />
            <span className="text-xs">View Services</span>
          </Button>
          <Button
            variant="outline"
            className="h-auto py-4 flex flex-col items-center gap-2 col-span-2"
            onClick={() => window.open('https://nolus.io', '_blank')}
          >
            <ExternalLink className="h-5 w-5" />
            <span className="text-xs">Nolus Website</span>
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
