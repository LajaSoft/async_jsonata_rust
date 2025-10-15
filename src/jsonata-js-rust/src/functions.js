(function () {
    'use strict';

    const native = require('../native/index.node');
    const upstreamFunctions = require('../../jsonata/src/functions');

    const rustFunctions = native.load_functions();

    const mergedFunctions = {
        ...upstreamFunctions,
        ...rustFunctions,
    };

    const rustSort = rustFunctions.sort;
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
