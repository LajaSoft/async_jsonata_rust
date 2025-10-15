(function () {
    'use strict';

    const native = require('../native/index.node');
    const upstreamFunctions = require('../../jsonata/src/functions');

    const rustFunctions = native.load_functions();

    const identifierPattern = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

    function extractParameters(fn) {
        if (typeof fn !== 'function') {
            return [];
        }
        const match = /\(([^)]*)\)/.exec(fn.toString());
        if (!match) {
            return [];
        }
        return match[1]
            .split(',')
            .map((arg) => arg.trim())
            .filter((arg) => arg.length > 0);
    }

    function wrapRustFunction(name, impl) {
        if (typeof impl !== 'function') {
            return impl;
        }

        const upstream = upstreamFunctions[name];
        const params = extractParameters(upstream);
        const argsList = params.join(', ');
        const factory = new Function(
            'impl',
            `return function(${argsList}) { return impl.apply(this, arguments); };`
        );
        const wrapped = factory(impl);

        try {
            Object.defineProperty(wrapped, 'name', {
                value: name,
                configurable: true,
            });
        } catch (err) {
            // ignore property definition issues
        }

        const displayName = identifierPattern.test(name) ? name : '';
        wrapped.toString = function () {
            const prefix = displayName ? `function ${displayName}` : 'function';
            return `${prefix}(${argsList}) { [native code] }`;
        };

        return wrapped;
    }

    const wrappedRustFunctions = Object.fromEntries(
        Object.entries(rustFunctions).map(([name, impl]) => [
            name,
            wrapRustFunction(name, impl),
        ])
    );

    const mergedFunctions = {
        ...upstreamFunctions,
        ...wrappedRustFunctions,
    };

    const rustSort = wrappedRustFunctions.sort;
    const jsSort = upstreamFunctions.sort;

    if (typeof rustSort === 'function' && typeof jsSort === 'function') {
        mergedFunctions.sort = function (...args) {
            const comparator = args[1];
            if (typeof comparator === 'function') {
                return Promise.resolve(jsSort.apply(this, args));
            }
            return Promise.resolve(rustSort.apply(this, args));
        };
    }

    module.exports = mergedFunctions;
}());
