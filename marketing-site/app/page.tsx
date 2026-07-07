import Navbar from "@/components/Navbar";
import Hero from "@/components/Hero";
import StatusBar from "@/components/StatusBar";
import Features from "@/components/Features";
import LogosMetrics from "@/components/LogosMetrics";
import Pricing from "@/components/Pricing";
import Faq from "@/components/Faq";
import Footer from "@/components/Footer";
import { getTeamsServed, getPlatformStatus } from "@/lib/data";

// Server Component: fetches live numbers on the server, streams static HTML.
export default async function Home() {
  const [teams, status] = await Promise.all([
    getTeamsServed(),
    getPlatformStatus(),
  ]);

  return (
    <>
      <Navbar />
      <main id="top">
        <Hero teamsServed={teams} />
        <StatusBar uptime={status.uptime} ok={status.ok} />
        <Features />
        <LogosMetrics />
        <Pricing />
        <Faq />
      </main>
      <Footer />
    </>
  );
}
