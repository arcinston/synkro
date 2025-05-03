import './App.css';
import { Onboarding } from './pages/Onboarding';
import { Toaster } from '@/components/ui/sonner'; // Assuming you've added toast

function App() {
  return (
    <main className="container">
      <Onboarding
        onComplete={() => {
          console.info('completed');
        }}
      />
      <Toaster />
    </main>
  );
}

export default App;
