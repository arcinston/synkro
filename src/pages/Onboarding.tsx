import { useState, useEffect } from 'react';
import { homeDir } from '@tauri-apps/api/path'; // Optional: To default dialog start location
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button'; // Adjust import path based on your setup
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { toast } from 'sonner';
import { load } from '@tauri-apps/plugin-store';
import type { Store } from '@tauri-apps/plugin-store';

interface OnboardingProps {
  onComplete: (selectedPath: string) => void; // Callback when setup is done
}

export const Onboarding = ({ onComplete }: OnboardingProps) => {
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [store, setStore] = useState<Store | null>(null);
  const [initialLoadComplete, setInitialLoadComplete] = useState(false);

  useEffect(() => {
    const initializeStore = async () => {
      try {
        const newStore = await load('store.json', { autoSave: false });
        setStore(newStore);

        const savedPath = await newStore.get<string>('sync-folder-path');
        setSelectedFolder(savedPath || null);
      } catch (error) {
        console.error('Error initializing store:', error);
        toast.error('Error Initializing Store', {
          description: 'Failed to initialize storage.',
        });
      } finally {
        setInitialLoadComplete(true); // Mark that the initial load is complete
      }
    };

    initializeStore();
  }, []); // Run only once on component mount

  const handleSelectFolder = async () => {
    setIsLoading(true);
    try {
      const defaultPath = await homeDir(); // Start dialog in user's home directory
      const selected = await open({
        directory: true, // We want to select a directory
        multiple: false, // Only allow selecting one directory
        title: 'Select Folder to Synchronize',
        defaultPath: defaultPath,
      });

      if (typeof selected === 'string') {
        // User selected a folder
        setSelectedFolder(selected);

        if (store) {
          await store.set('sync-folder-path', selected);
          await store.save(); // Save the changes
        } else {
          console.error('Store not initialized yet.');
          toast.error('Store Error', {
            description: 'Could not save the selected path.',
          });
        }
      } else if (selected === null) {
        // User cancelled the dialog
        console.info('Folder selection cancelled.');
      }
      // `result` could also be string[] if multiple: true, but we set it to false
    } catch (error) {
      console.error('Error selecting folder:', error);
      toast.error('Error Selecting Folder', {
        description: 'Could not open the folder selection dialog.',
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleContinue = () => {
    if (selectedFolder) {
      console.info('Proceeding with folder:', selectedFolder);
      // Here you would typically save this path persistently
      // e.g., using tauri-plugin-store or sending it to the Rust backend
      onComplete(selectedFolder);
    } else {
      toast.info('No Folder Selected', {
        description: 'Please select a folder first.',
      });
    }
  };

  // Wait for initial data loading before rendering the main content
  if (!initialLoadComplete) {
    return <div>Loading...</div>; // Or any other loading indicator
  }

  return (
    <div className="flex items-center justify-center min-h-screen bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="text-2xl">Say Hi to Sinky!</CardTitle>
          <CardDescription>
            Let's set up the folder you want to keep synchronized across your
            devices.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            Choose an existing folder or create a new one. All files and
            subfolders within this location will be synced.
          </p>
          <div>
            <Button
              onClick={handleSelectFolder}
              disabled={isLoading}
              variant="outline"
              className="w-full"
            >
              {isLoading ? 'Opening...' : 'Select Sync Folder'}
            </Button>
          </div>
          {selectedFolder && (
            <div className="space-y-1">
              <label
                htmlFor="selected-path"
                className="text-sm font-medium text-muted-foreground"
              >
                Selected Path:
              </label>
              {/* Using Input visually looks consistent, but disable it */}
              <Input
                id="selected-path"
                type="text"
                value={selectedFolder}
                readOnly
                disabled
                className="text-xs cursor-default" // Make it look clearly non-interactive
              />
            </div>
          )}
        </CardContent>
        <CardFooter>
          <Button
            className="w-full"
            onClick={handleContinue}
            disabled={!selectedFolder || isLoading}
          >
            Continue
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
};
