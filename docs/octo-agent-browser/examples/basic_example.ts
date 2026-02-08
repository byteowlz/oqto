import { AgentBrowser } from '../src';

/**
 * Basic example of using Octo Agent Browser
 */
async function basicExample() {
  // Create a new agent browser instance
  const browser = new AgentBrowser({
    headless: false, // Set to true for headless mode
    timeout: 30000,
  });

  try {
    // Navigate to a website
    await browser.navigate('https://example.com');

    // Get page title
    const title = await browser.getTitle();
    console.log('Page title:', title);

    // Extract text content
    const text = await browser.getText('body');
    console.log('Page text (first 200 chars):', text.substring(0, 200));

    // Take a screenshot
    const screenshot = await browser.screenshot();
    console.log('Screenshot saved:', screenshot);

    // Click on an element
    await browser.click('a');

    // Wait for navigation
    await browser.waitForNavigation();

    // Fill a form
    await browser.fill('#username', 'testuser');
    await browser.fill('#password', 'testpass');

    // Submit the form
    await browser.submit('form');

  } finally {
    // Always close the browser
    await browser.close();
  }
}

// Run the example
basicExample().catch(console.error);
