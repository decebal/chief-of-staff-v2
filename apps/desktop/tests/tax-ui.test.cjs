const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

test('tax dashboard handles missing values, projections and saving', async () => {
  class Element {
    constructor() { this.value = ''; this.dataset = {}; this.style = {}; this.children = []; this.listeners = {}; }
    set value(value) { this._value = String(value); }
    get value() { return this._value; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    setAttribute() {}
    addEventListener(name, fn) { this.listeners[name] = fn; }
    reportValidity() { return true; }
  }
  const nodes = new Map();
  const $ = (id) => { if (!nodes.has(id)) nodes.set(id, new Element()); return nodes.get(id); };
  const tab = new Element();
  let saved;
  const context = vm.createContext({
    $, Intl, Object, Number, Math,
    document: { querySelector: () => tab },
    el: (_tag, props = {}, ...children) => { const node = new Element(); Object.assign(node, props); node.append(...children); return node; },
    invoke: async (name, args) => { if (name === 'tax_read') return null; if (name === 'tax_save') saved = args.data; },
    logDev: () => {},
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../ui/tax.js'), 'utf8'), context);
  await vm.runInContext('loadTax()', context);
  assert.equal($('tax-rent').value, '');
  assert.equal($('tax-metrics').children[0].children[1].textContent, '—');
  for (const [field, value] of Object.entries({ 'partner-salary': '36000', 'partner-property': '1000', 'partner-other': '0', 'partner-dividend': '14000', 'owner-income': '102000', 'loan-balance': '40000', repayment: '10000', rent: '900' })) $(`tax-${field}`).value = value;
  vm.runInContext('renderTax()', context);
  assert.equal($('tax-metrics').children[0].children[1].textContent, '£0.00');
  assert.equal($('tax-metrics').children[2].children[1].textContent, '£30,000.00');
  assert.match($('tax-charts').children[3].textContent, /£1,000.00/);
  assert.match($('tax-loan').children[0].textContent, /£10,725.00/);
  await $('tax-form').listeners.submit({ preventDefault() {} });
  assert.equal(saved.values['partner-salary'], 36000);
  assert.equal(saved.values.rent, 900);
});
