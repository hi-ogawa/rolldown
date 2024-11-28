var rolldown_runtime = self.rolldown_runtime = {
  patching: false,
  patchedModuleFactoryMap: {},
  executeModuleStack: [],
  moduleCache: {},
  moduleFactoryMap: {},
  define: function (id, factory) {
    if (self.patching) {
      this.patchedModuleFactoryMap[id] = factory;
    } else {
      this.moduleFactoryMap[id] = factory;
    }
  },
  require: function (id) {
    const parent = this.executeModuleStack.length > 1 ? this.executeModuleStack[this.executeModuleStack.length - 1] : null;
    if (this.moduleCache[id]) {
      var module = this.moduleCache[id];
      if(module.parents.indexOf(parent) === -1) {
        module.parents.push(parent);
      }
      return module.exports;
    }
    var factory = this.moduleFactoryMap[id];
    if (!factory) {
      throw new Error('Module not found: ' + id);
    }
    var module = this.moduleCache[id] = {
      exports: {},
      parents: [parent],
      hot: {
        selfAccept: false,
        acceptCallbacks: [],
        accept: function(callback) {
          this.selfAccept = true;
          if(callback && typeof callback === 'function') {
            this.acceptCallbacks.push({
              deps: [id],
              callback
            });
          }
        }
      }
    };
    this.executeModuleStack.push(id);
    factory(this.require.bind(this), module, module.exports);
    this.executeModuleStack.pop();
    return module.exports;
  },
  patch: function(updateModuleIds, callback) {
    self.patching = true;

    callback();

    var boundaries = [];
    var invalidModuleIds = [];
    var acceptCallbacks = [];

    for (var i = 0; i < updateModuleIds.length; i++) {
      foundBoundariesAndInvalidModuleIds(updateModuleIds[i], boundaries, invalidModuleIds, acceptCallbacks)
    }

    for (var i = 0; i < invalidModuleIds.length; i++) {
      var id = invalidModuleIds[i];
      delete this.moduleCache[id];
    }

    for (var id in this.patchedModuleFactoryMap) {
      this.moduleFactoryMap[id] = this.patchedModuleFactoryMap[id];
    }
    this.patchedModuleFactoryMap = {}

    for (var i = 0; i < boundaries.length; i++) {
      this.require(boundaries[i]);
    }

    for (var i = 0; i < acceptCallbacks.length; i++) {
      var item = acceptCallbacks[i];
      item.callback.apply(null, item.deps.map((dep) => this.moduleCache[dep].exports));
    }

    self.patching = false;

    function foundBoundariesAndInvalidModuleIds(updateModuleId, boundaries, invalidModuleIds, acceptCallbacks) {
      var queue = [ { moduleId: updateModuleId, chain: [updateModuleId] }];
      var visited = {};

      while (queue.length > 0) {
        var item = queue.pop();
        var moduleId = item.moduleId;
        var chain = item.chain;

        if (visited[moduleId]) {
          continue;
        }

        var module = rolldown_runtime.moduleCache[moduleId];

        if (module.hot.selfAccept) {
          if(boundaries.indexOf(moduleId) === -1) {
            boundaries.push(moduleId);

            for (var i = 0; i < module.hot.acceptCallbacks.length; i++) {
              var item = module.hot.acceptCallbacks[i];
              if(item.deps.includes(updateModuleId)) {
                acceptCallbacks.push(item);
              }
            }
          }
          for (var i = 0; i < chain.length; i++) {
            if(invalidModuleIds.indexOf(chain[i]) === -1) {
              invalidModuleIds.push(chain[i]);
            }
          }
          continue;
        }

        for(var i = 0; i < module.parents.length; i++) {
          var parent = module.parents[i];
          queue.push({
            moduleId: parent,
            chain: chain.concat([parent])
          });
        }

        visited[moduleId] = true;
      }


    }
  }
  ,
  loadScript: function (url) {
    var script = document.createElement('script');
    script.src = url;
    script.onerror = function() {
      console.error('Failed to load script: ' + url);
    }
    document.body.appendChild(script);
  }
}

const socket = new WebSocket(`ws://localhost:8080`)

socket.onmessage = function(event) {
  const data = JSON.parse(event.data)
  if (data.type === 'update') {
    rolldown_runtime.loadScript(data.url)
  }
}
