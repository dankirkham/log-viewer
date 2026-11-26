import argparse

from selenium import webdriver
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.chrome.service import Service
from selenium.webdriver.common.by import By
from selenium.webdriver.support.ui import WebDriverWait
from selenium.common.exceptions import TimeoutException


def read_state(driver):
    return driver.execute_script(
        """const e = document.getElementById('log-viewer-smoke');
        if (!e) return null;
        const out = {};
        for (const key of ['loaded','filename','sample-index','sample-count','rpm',
          'afr-visible','ve-visible','afr-diagnostic','ve-diagnostic','error','loading']) out[key] = e.getAttribute(key);
        out['load-ms'] = e.getAttribute('load-ms');
        return out;"""
    )


def wait_state(driver, predicate, message, timeout=20):
    return WebDriverWait(driver, timeout).until(
        lambda d: read_state(d) if predicate(read_state(d)) else False,
        message,
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', required=True)
    parser.add_argument('--browser', required=True)
    parser.add_argument('--driver', required=True)
    parser.add_argument('--fixture-url', required=True)
    parser.add_argument('--fixture-name', required=True)
    parser.add_argument('--expect-samples', type=int, default=3)
    parser.add_argument('--expect-first-rpm', type=float, default=1000.0)
    parser.add_argument('--expect-last-rpm', type=float, default=1200.0)
    args = parser.parse_args()

    options = Options()
    options.binary_location = args.browser
    options.add_argument('--headless=new')
    options.add_argument('--no-sandbox')
    options.add_argument('--disable-dev-shm-usage')
    options.add_argument('--disable-gpu')
    options.add_argument('--window-size=1440,1000')
    options.set_capability('goog:loggingPrefs', {'browser': 'ALL'})
    driver = webdriver.Chrome(service=Service(args.driver), options=options)
    try:
        driver.execute_cdp_cmd(
            'Page.addScriptToEvaluateOnNewDocument',
            {'source': "window.__fatalErrors=[]; window.addEventListener('error',e=>window.__fatalErrors.push(String(e.message||e.error))); window.addEventListener('unhandledrejection',e=>window.__fatalErrors.push(String(e.reason)));"},
        )
        driver.set_page_load_timeout(30)
        driver.get(args.url)
        WebDriverWait(driver, 30).until(
            lambda d: d.execute_script("return !!document.querySelector('#app')")
        )
        wait_state(driver, lambda state: state is not None, 'egui smoke state did not initialize')

        driver.execute_script(
            """const path=arguments[0], name=arguments[1];
            window.__logViewerSmokeError='';
            const started=performance.now();
            fetch(path).then(response => {
              if (!response.ok) throw new Error(`fixture request failed: ${response.status}`);
              return response.arrayBuffer();
            }).then(bytes => {
              if (!window.__logViewerSmokeImport) throw new Error('smoke import hook missing');
              window.__logViewerSmokeImport(name, bytes, started);
            }).catch(error => { window.__logViewerSmokeError=String(error); });""",
            args.fixture_url,
            args.fixture_name,
        )
        try:
            state = wait_state(
                driver,
                lambda s: s and (
                    (s['loaded'] == 'true' and s['loading'] == 'false')
                    or s['error']
                    or driver.execute_script('return window.__logViewerSmokeError')
                ),
                'fixture import did not finish in the browser',
                timeout=30,
            )
        except TimeoutException:
            print(f'Browser import state: {read_state(driver)}')
            print(f'Browser console: {driver.get_log("browser")}')
            raise
        assert state['loaded'] == 'true', (
            state,
            driver.execute_script('return window.__logViewerSmokeError'),
        )
        expected_samples = args.expect_samples
        assert state['filename'] == args.fixture_name, state
        assert state['sample-index'] == '0', state
        assert int(state['sample-count']) == expected_samples, state
        assert float(state['rpm']) == args.expect_first_rpm, state
        assert float(state['load-ms']) >= 0.0, state

        driver.execute_script(
            """const canvas = document.getElementById('app'); canvas.focus();
            canvas.dispatchEvent(new KeyboardEvent('keydown', {key:'End', code:'End',
              bubbles:true, cancelable:true}));
            canvas.dispatchEvent(new KeyboardEvent('keyup', {key:'End', code:'End',
              bubbles:true, cancelable:true}));"""
        )
        state = wait_state(
            driver,
            lambda s: s and s['sample-index'] == str(expected_samples - 1),
            'End seek did not select the final recorded row',
        )
        assert float(state['rpm']) == args.expect_last_rpm, state

        state = wait_state(
            driver,
            lambda s: s and s['afr-visible'] == 'true' and s['ve-visible'] == 'true',
            'dashboard map panels were not rendered',
        )
        assert state['afr-diagnostic'] and state['afr-diagnostic'] != 'not opened', state
        assert state['ve-diagnostic'] and state['ve-diagnostic'] != 'not opened', state

        fatal = driver.execute_script('return window.__fatalErrors')
        browser_errors = [
            entry['message'] for entry in driver.get_log('browser')
            if entry['level'] == 'SEVERE'
        ]
        assert not fatal, fatal
        assert not browser_errors, browser_errors
        print(
            f'Browser smoke passed: {state["sample-count"]} rows, End selected row '
            f'{expected_samples - 1} (RPM {state["rpm"]}), dashboard map panels rendered; '
            f'local fetch + parse {state["load-ms"]} ms.'
        )
    finally:
        driver.quit()


if __name__ == '__main__':
    main()
