import { useState, useEffect } from 'react';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  Button,
  ScrollArea,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@kostovster/ui';
import { Download, HardDrive, Copy } from 'lucide-react';
import { toast } from 'sonner';

interface SnapshotInfo {
  node_name: string;
  network: string;
  filename: string;
  snapshot_path: string;
  file_size_bytes?: number;
}

interface SnapshotDownloadsProps {
  nodeNames: string[];
  isLoading?: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function SnapshotDownloads({ nodeNames, isLoading = false }: SnapshotDownloadsProps) {
  const [snapshots, setSnapshots] = useState<SnapshotInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchLatestSnapshots() {
      if (nodeNames.length === 0) {
        setLoading(false);
        return;
      }

      try {
        const results = await Promise.all(
          nodeNames.map(async (name) => {
            try {
              const response = await fetch(`/api/snapshots/${name}/list`);
              if (!response.ok) return null;
              const data = await response.json();
              if (data.success && data.data?.length > 0) {
                // Get the latest snapshot (first in list, sorted by date)
                return data.data[0] as SnapshotInfo;
              }
              return null;
            } catch {
              return null;
            }
          })
        );

        // Group by network and keep only latest per network
        const byNetwork = new Map<string, SnapshotInfo>();
        results.filter(Boolean).forEach((snapshot) => {
          if (snapshot) {
            const existing = byNetwork.get(snapshot.network);
            if (!existing || snapshot.filename > existing.filename) {
              byNetwork.set(snapshot.network, snapshot);
            }
          }
        });

        setSnapshots(Array.from(byNetwork.values()));
      } catch (error) {
        console.error('Failed to fetch snapshots:', error);
      } finally {
        setLoading(false);
      }
    }

    fetchLatestSnapshots();
  }, [nodeNames]);

  const showLoading = isLoading || loading;

  if (showLoading) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <Download className="h-5 w-5" />
            Latest Snapshots
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="flex items-center justify-between p-3 rounded-lg border">
                <div className="space-y-1">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-3 w-32" />
                </div>
                <Skeleton className="h-8 w-8" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (snapshots.length === 0) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <Download className="h-5 w-5" />
            Latest Snapshots
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="py-6 text-center text-muted-foreground">
            <HardDrive className="h-10 w-10 mx-auto mb-3 opacity-50" />
            <p className="font-medium">No snapshots available</p>
            <p className="text-sm">Create snapshots to see download links</p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-base">
            <Download className="h-5 w-5" />
            Latest Snapshots
          </CardTitle>
          <Badge variant="secondary" className="text-xs">
            {snapshots.length} networks
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <ScrollArea className="h-64">
          <div className="space-y-2 pr-4">
            {snapshots.map((snapshot) => (
              <div
                key={snapshot.network}
                className="flex items-center justify-between p-3 rounded-lg border hover:bg-muted/50 transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm">{snapshot.network}</span>
                    {snapshot.file_size_bytes && (
                      <Badge variant="outline" className="text-xs">
                        {formatBytes(snapshot.file_size_bytes)}
                      </Badge>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground truncate mt-0.5">
                    {snapshot.filename}
                  </p>
                </div>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="outline"
                      size="icon"
                      onClick={() => {
                        navigator.clipboard.writeText(snapshot.snapshot_path);
                        toast.success('Path copied to clipboard');
                      }}
                    >
                      <Copy className="h-4 w-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Copy snapshot path</p>
                  </TooltipContent>
                </Tooltip>
              </div>
            ))}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}
