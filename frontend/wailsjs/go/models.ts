export namespace main {
	
	export class Event {
	    event_type: string;
	    ip: string;
	    old_mac?: string;
	    new_mac?: string;
	    mac?: string;
	    timestamp: number;
	
	    static createFrom(source: any = {}) {
	        return new Event(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.event_type = source["event_type"];
	        this.ip = source["ip"];
	        this.old_mac = source["old_mac"];
	        this.new_mac = source["new_mac"];
	        this.mac = source["mac"];
	        this.timestamp = source["timestamp"];
	    }
	}

}

