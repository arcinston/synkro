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
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'; // Added for gossip options
import { Label } from '@/components/ui/label'; // Added for RadioGroup labels
import { invoke } from '@tauri-apps/api/core';

interface OnboardingProps {
  // Callback when setup is done, now includes gossip ticket info
  onComplete: (
    selectedPath: string,
    gossipTicket: string | null,
    isGeneratingNewTicket: boolean,
  ) => void;
}

export const Onboarding = ({ onComplete }: OnboardingProps) => {
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false); // For folder selection
  const [store, setStore] = useState<Store | null>(null);
  const [initialLoadComplete, setInitialLoadComplete] = useState(false);

  // New state for multi-step onboarding and gossip ticket
  const [onboardingStep, setOnboardingStep] = useState<
    'folderSelection' | 'gossipSetup'
  >('folderSelection');
  const [gossipOption, setGossipOption] = useState<'generate' | 'input' | null>(
    null,
  );
  const [generatedGossipTicket, setGeneratedGossipTicket] = useState<
    string | null
  >(null);
  const [inputGossipTicket, setInputGossipTicket] = useState<string>('');
  const [isGossipLoading, setIsGossipLoading] = useState(false); // For gossip operations

  useEffect(() => {
    const initializeStore = async () => {
      try {
        const newStore = await load('store.json', { autoSave: false });
        setStore(newStore);

        const savedPath = await newStore.get<string>('sync-folder-path');
        setSelectedFolder(savedPath || null);

        // Load gossip ticket information
        const savedGossipTicket = await newStore.get<string>(
          'gossip-topic-ticket',
        );
        // const savedGossipTicketType = await newStore.get<'generate' | 'input'>('gossip-ticket-type'); // Optional: if needed for more complex logic

        if (savedGossipTicket) {
          setInputGossipTicket(savedGossipTicket);
          setGossipOption('input'); // Default to 'input' mode if a ticket exists
          // If you also want to display it in the "generated" field if it was originally generated,
          // you might need more complex logic or rely on 'savedGossipTicketType'.
          // For now, per request, it defaults to the input field.
        }
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
      console.info('Default path:', defaultPath);
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

  // Renamed to be specific to folder selection step
  const handleContinueAfterFolderSelect = () => {
    if (selectedFolder) {
      invoke('setup_iroh_and_fs');
      console.info('Proceeding to gossip setup with folder:', selectedFolder);
      setOnboardingStep('gossipSetup'); // Transition to the next step
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

  // --- Gossip Ticket Logic ---
  const handleGenerateTicket = async () => {
    setIsGossipLoading(true);
    setGeneratedGossipTicket(null); // Clear previous if any
    setInputGossipTicket(''); // Clear input field
    const ticket = await invoke<string>('create_gossip_ticket');
    setGeneratedGossipTicket(ticket);
    setGossipOption('generate'); // Ensure this option is selected
    toast.success('Gossip Ticket Generated', {
      description: 'You can share this with other devices.',
    });
    setIsGossipLoading(false);
  };

  const handleFinishSetup = async () => {
    if (!selectedFolder) {
      // Should not happen if flow is correct, but good check
      toast.error('Error', { description: 'Folder not selected.' });
      setOnboardingStep('folderSelection'); // Go back
      return;
    }

    if (!store) {
      toast.error('Error', { description: 'Tauri Store not mounted' });
      setOnboardingStep('folderSelection'); // Go back
      return;
    }

    if (gossipOption === 'generate' && generatedGossipTicket) {
      onComplete(selectedFolder, generatedGossipTicket, true);
      const response = await invoke<boolean>('join_gossip', {
        strGossipTicket: generatedGossipTicket,
      });
      console.info('join gossip :', response);
      await store.set('gossip-topic-ticket', generatedGossipTicket);
      await store.save();
    } else if (gossipOption === 'input' && inputGossipTicket.trim() !== '') {
      onComplete(selectedFolder, inputGossipTicket.trim(), false);
      const response = await invoke<boolean>('join_gossip', {
        strGossipTicket: inputGossipTicket,
      });
      console.info('join gossip :', response);
      await store.set('gossip-topic-ticket', inputGossipTicket.trim());
      await store.save();
    } else {
      toast.info('Gossip Configuration Incomplete', {
        description:
          'Please generate a new ticket or input an existing one to join a sync group.',
      });
    }
  };

  return (
    <div className="flex items-center justify-center min-h-screen bg-background p-4">
      {onboardingStep === 'folderSelection' && (
        <Card className="w-full max-w-md">
          <CardHeader>
            <CardTitle className="text-2xl">Welcome to Synkro!</CardTitle>
            <CardDescription>
              First, let's choose a folder to synchronize.
            </CardDescription>
            <CardDescription className="text-xs text-muted-foreground">
              Powered by Tauri & Iroh (P2P Magic)
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              All files and subfolders within this location will be synced.
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
                <Label htmlFor="selected-path" className="text-sm font-medium">
                  Selected Path:
                </Label>
                <Input
                  id="selected-path"
                  type="text"
                  value={selectedFolder}
                  readOnly
                  disabled
                  className="text-xs cursor-default"
                />
              </div>
            )}
          </CardContent>
          <CardFooter>
            <Button
              className="w-full"
              onClick={handleContinueAfterFolderSelect}
              disabled={!selectedFolder || isLoading}
            >
              Continue
            </Button>
          </CardFooter>
        </Card>
      )}

      {onboardingStep === 'gossipSetup' && (
        <Card className="w-full max-w-md">
          <CardHeader>
            <CardTitle className="text-2xl">
              Join or Create a Sync Group
            </CardTitle>
            <CardDescription>
              To sync with other devices, you need a Gossip Ticket. You can
              generate a new one or use an existing one.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-6">
            <RadioGroup
              value={gossipOption || ''}
              onValueChange={(value: 'generate' | 'input') => {
                setGossipOption(value);
                if (value === 'input') setGeneratedGossipTicket(null); // Clear generated if switching to input
              }}
              className="space-y-2"
            >
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="generate" id="r-generate" />
                <Label htmlFor="r-generate">Generate a new Gossip Ticket</Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="input" id="r-input" />
                <Label htmlFor="r-input">Input an existing Gossip Ticket</Label>
              </div>
            </RadioGroup>

            {gossipOption === 'generate' && (
              <div className="space-y-2">
                <Button
                  onClick={handleGenerateTicket}
                  disabled={isGossipLoading}
                  variant="secondary"
                  className="w-full"
                >
                  {isGossipLoading ? 'Generating...' : 'Generate New Ticket'}
                </Button>
                {generatedGossipTicket && (
                  <div>
                    <Label
                      htmlFor="generated-ticket"
                      className="text-sm font-medium"
                    >
                      Your New Ticket (Share this):
                    </Label>
                    <Input
                      id="generated-ticket"
                      type="text"
                      value={generatedGossipTicket}
                      readOnly
                      className="text-xs font-mono cursor-copy"
                      onClick={() => {
                        navigator.clipboard.writeText(generatedGossipTicket);
                        toast.success('Ticket copied to clipboard!');
                      }}
                    />
                  </div>
                )}
              </div>
            )}

            {gossipOption === 'input' && (
              <div className="space-y-2">
                <Label htmlFor="input-ticket" className="text-sm font-medium">
                  Paste Gossip Ticket:
                </Label>
                <Input
                  id="input-ticket"
                  type="text"
                  placeholder="Enter Gossip Ticket from another device"
                  value={inputGossipTicket}
                  onChange={(e) => setInputGossipTicket(e.target.value)}
                  disabled={isGossipLoading}
                />
              </div>
            )}
          </CardContent>
          <CardFooter className="flex flex-col sm:flex-row gap-2">
            <Button
              variant="outline"
              onClick={() => setOnboardingStep('folderSelection')}
              className="w-full sm:w-auto"
            >
              Back
            </Button>
            <Button
              className="w-full sm:flex-grow"
              onClick={handleFinishSetup}
              disabled={
                isGossipLoading ||
                !gossipOption ||
                (gossipOption === 'generate' && !generatedGossipTicket) ||
                (gossipOption === 'input' && inputGossipTicket.trim() === '')
              }
            >
              Finish Setup & Start Syncing
            </Button>
          </CardFooter>
        </Card>
      )}
    </div>
  );
};
