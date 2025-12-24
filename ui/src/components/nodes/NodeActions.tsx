import { useState } from 'react';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@kostovster/ui';
import { MoreHorizontal, Scissors, Camera, RotateCcw, RefreshCw, Play } from 'lucide-react';
import { toast } from 'sonner';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { pruneNode, createSnapshot, restoreSnapshot, executeStateSync, restartNode } from '@/api/client';
import { formatName } from '@/lib/utils';
import type { NodeConfig } from '@/types';

interface NodeActionsProps {
  nodeName: string;
  config: NodeConfig;
  onActionComplete: () => void;
}

type ActionType = 'restart' | 'prune' | 'snapshot' | 'restore' | 'stateSync' | null;

export function NodeActions({ nodeName, config, onActionComplete }: NodeActionsProps) {
  const [confirmAction, setConfirmAction] = useState<ActionType>(null);
  const [isLoading, setIsLoading] = useState(false);

  const displayName = formatName(nodeName);

  const actionConfigs: Record<NonNullable<ActionType>, {
    title: string;
    description: string;
    confirmText: string;
    action: () => Promise<{ success: boolean; message?: string }>;
    successMessage: string;
    variant?: 'default' | 'destructive';
  }> = {
    restart: {
      title: 'Restart Node',
      description: `Restart ${displayName}?\n\nThis will stop and start the blockchain node service.\n\nProceed with restart?`,
      confirmText: 'Restart',
      action: () => restartNode(nodeName),
      successMessage: `${displayName} restarted successfully`,
      variant: 'default',
    },
    prune: {
      title: 'Execute Pruning',
      description: `Start pruning for ${displayName}?\n\nThis will:\n- Stop the node service\n- Run the cosmos-pruner tool\n- Start the service after completion\n- May take several hours\n\nProceed with pruning?`,
      confirmText: 'Start Pruning',
      action: () => pruneNode(nodeName),
      successMessage: `Pruning started for ${displayName}`,
      variant: 'default',
    },
    snapshot: {
      title: 'Create Snapshot',
      description: `Create snapshot for ${displayName}?\n\nThis will:\n- Stop the node service\n- Compress data directories\n- Restart the service\n- May take 30-120 minutes\n\nProceed with snapshot creation?`,
      confirmText: 'Create Snapshot',
      action: () => createSnapshot(nodeName),
      successMessage: `Snapshot creation started for ${displayName}`,
      variant: 'default',
    },
    restore: {
      title: 'Restore from Snapshot',
      description: `Restore ${displayName} from latest snapshot?\n\nWARNING: This will:\n- Stop the node service\n- Delete ALL current data & wasm directories\n- Extract latest snapshot\n- Restore validator state\n- Start the service\n\nThis action CANNOT BE UNDONE.\n\nProceed with restore?`,
      confirmText: 'Restore',
      action: () => restoreSnapshot(nodeName),
      successMessage: `${displayName} restore started`,
      variant: 'destructive',
    },
    stateSync: {
      title: 'Execute State Sync',
      description: `Execute state sync for ${displayName}?\n\nWARNING: This will:\n- Stop the node service\n- Fetch trusted height from RPC\n- Execute unsafe-reset-all (WIPES ALL DATA)\n- Clean WASM cache\n- Start service and wait for sync\n\nThis action CANNOT BE UNDONE.\n\nProceed with state sync?`,
      confirmText: 'Execute State Sync',
      action: () => executeStateSync(nodeName),
      successMessage: `State sync started for ${displayName}`,
      variant: 'destructive',
    },
  };

  const handleConfirm = async () => {
    if (!confirmAction) return;

    const actionConfig = actionConfigs[confirmAction];
    setIsLoading(true);

    try {
      const response = await actionConfig.action();
      if (response.success) {
        toast.success(actionConfig.successMessage);
        onActionComplete();
      } else {
        throw new Error(response.message || 'Operation failed');
      }
    } catch (error) {
      toast.error(`Failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsLoading(false);
      setConfirmAction(null);
    }
  };

  const hasAnyAction = config.pruning_enabled || config.snapshots_enabled || 
                       config.auto_restore_enabled || config.state_sync_enabled;

  // Only restart available - show single icon button
  if (!hasAnyAction) {
    return (
      <>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="icon" onClick={() => setConfirmAction('restart')}>
              <Play className="h-4 w-4" />
              <span className="sr-only">Restart</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            <p>Restart Node</p>
          </TooltipContent>
        </Tooltip>

        {confirmAction && (
          <ConfirmDialog
            open={!!confirmAction}
            onOpenChange={(open) => !open && !isLoading && setConfirmAction(null)}
            title={actionConfigs[confirmAction].title}
            description={actionConfigs[confirmAction].description}
            confirmText={isLoading ? 'Processing...' : actionConfigs[confirmAction].confirmText}
            onConfirm={handleConfirm}
            variant={actionConfigs[confirmAction].variant}
          />
        )}
      </>
    );
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" size="icon">
            <MoreHorizontal className="h-4 w-4" />
            <span className="sr-only">Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuLabel>Node Actions</DropdownMenuLabel>
          <DropdownMenuSeparator />
          
          <DropdownMenuItem onClick={() => setConfirmAction('restart')}>
            <Play className="h-4 w-4 mr-2" />
            Restart Node
          </DropdownMenuItem>
          
          {config.pruning_enabled && (
            <DropdownMenuItem onClick={() => setConfirmAction('prune')}>
              <Scissors className="h-4 w-4 mr-2" />
              Execute Pruning
            </DropdownMenuItem>
          )}
          
          {config.snapshots_enabled && (
            <DropdownMenuItem onClick={() => setConfirmAction('snapshot')}>
              <Camera className="h-4 w-4 mr-2" />
              Create Snapshot
            </DropdownMenuItem>
          )}
          
          {config.auto_restore_enabled && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem 
                onClick={() => setConfirmAction('restore')}
                className="text-destructive focus:text-destructive"
              >
                <RotateCcw className="h-4 w-4 mr-2" />
                Restore from Snapshot
              </DropdownMenuItem>
            </>
          )}
          
          {config.state_sync_enabled && (
            <DropdownMenuItem 
              onClick={() => setConfirmAction('stateSync')}
              className="text-destructive focus:text-destructive"
            >
              <RefreshCw className="h-4 w-4 mr-2" />
              Execute State Sync
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {confirmAction && (
        <ConfirmDialog
          open={!!confirmAction}
          onOpenChange={(open) => !open && !isLoading && setConfirmAction(null)}
          title={actionConfigs[confirmAction].title}
          description={actionConfigs[confirmAction].description}
          confirmText={isLoading ? 'Processing...' : actionConfigs[confirmAction].confirmText}
          onConfirm={handleConfirm}
          variant={actionConfigs[confirmAction].variant}
        />
      )}
    </>
  );
}
