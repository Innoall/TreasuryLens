#!/usr/bin/env python3
"""Real four-node integration checks. Keep localnet + deployed app + serve running.
Use --faults ONLY with the explicit treasurylens-test deployment.
"""
import argparse, json, os, subprocess, sys, time, urllib.request, urllib.error
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
ENV=os.environ.copy();ENV['PATH']=str(ROOT/'.tools/bin')+os.pathsep+ENV.get('PATH','')

def request(url,payload=None,timeout=35):
    data=None if payload is None else json.dumps(payload).encode()
    req=urllib.request.Request(url,data,{'content-type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=timeout) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)

def rpc(method,params):
    return request('http://127.0.0.1:8545',{'jsonrpc':'2.0','id':1,'method':method,'params':params})[1]['result']

def cast(*args,check=True):
    r=subprocess.run(['cast',*args],cwd=ROOT,env=ENV,text=True,capture_output=True,timeout=35)
    if check and r.returncode:raise AssertionError(r.stderr)
    return r

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url',default='http://127.0.0.1:8080')
    parser.add_argument('--faults',action='store_true')
    args=parser.parse_args();base=args.base_url.rstrip('/')
    seed=json.loads((ROOT/'.runtime/seed.json').read_text())
    dep=json.loads((ROOT/'.runtime/deployment.json').read_text())
    rows=[]
    def case(name,fn):
        start=time.monotonic()
        try:
            result=fn();rows.append({'case':name,'passed':True,'elapsed_ms':round((time.monotonic()-start)*1000,2),'evidence':result})
            print('PASS',name,flush=True)
        except Exception as e:
            rows.append({'case':name,'passed':False,'error':str(e)});print('FAIL',name,str(e),flush=True)
    def query(q,expected_total,expected_count,expect_success=True):
        code,body=request(base+'/api/query',q)
        if not expect_success:
            assert code>=400 and body['status']=='failed',(code,body)
            return body
        assert code in (200,202), (code,body)
        query_id=body['query_id']
        deadline=time.monotonic()+25
        record=body.get('record')
        while record is None and time.monotonic()<deadline:
            records=request(base+'/api/results')[1]['results']
            record=next((r for r in records if r['summary']['query_id']==query_id),None)
            if record is None:time.sleep(.25)
        assert record is not None,'certified submission never reached shared state'
        assert record['summary']['total']==str(expected_total),record
        assert record['summary']['transfer_count']==expected_count,record
        assert len(set(record['supporters']))>=3 and len(set(record['certifiers']))>=3,record
        assert set(record['supporters']).issubset(dep['committee']),record
        assert set(record['certifiers']).issubset(dep['committee']),record
        assert record['summary']['snapshot_hash']==record['query']['snapshot_hash'],record
        return {'query_id':query_id,'summary':record['summary'],'supporters':record['supporters'],'certifiers':record['certifiers']}
    q={k:seed[k] for k in ['token','treasury','from_block','to_block','budget']}
    def health():
        code,body=request(base+'/api/status')
        assert code==200 and body['active'] and body['threshold']==3 and len(set(body['committee']))==4,body
        assert body['lyquid_id']==dep['lyquid_id'],body
        return body
    case('four_node_committee_active',health)
    case('known_outflow_excludes_inbound_and_unrelated',lambda:query(q,seed['expected_total'],seed['expected_count']))
    case('idempotent_query',lambda:query(q,seed['expected_total'],seed['expected_count']))
    empty=dict(q);empty['from_block']=seed['to_block']+1;empty['to_block']=empty['from_block']
    # This block is one of the empty successor blocks explicitly mined by the seed script.
    case('empty_interval_confirms_zero',lambda:query(empty,0,0))
    bad=dict(q);bad['token']='not-an-address'
    case('invalid_address_rejected',lambda:query(bad,0,0,False))
    bad_range=dict(q);bad_range['from_block']=bad_range['to_block']+1
    case('reversed_range_rejected',lambda:query(bad_range,0,0,False))
    def future_interval():
        head=int(rpc('eth_blockNumber',[]),16)
        future=dict(q);future['from_block']=head+100;future['to_block']=head+100
        code,body=request(base+'/api/query',future)
        assert code==422 and body.get('status')=='failed',(code,body)
        assert 'successor' in body.get('error','').lower(),body
        return {'future_block':future['to_block'],'source_head_before_request':head,
                'error':body['error'],'status':body['status']}
    case('future_interval_requires_successor_blocks',future_interval)
    eoa=dict(q);eoa['token']=seed['treasury']
    case('noncontract_token_rejected',lambda:query(eoa,0,0,False))
    def unauthorized():
        key='0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d'
        r=cast('send','--no-proxy','--json','--rpc-url',dep['endpoint'],'--private-key',key,
            '--gas-limit','1000000',dep['contract'],'set_defaults(string,string,uint64,uint64,string)',
            q['token'],q['treasury'],str(q['from_block']),str(q['to_block']),'1',check=False)
        if r.returncode==0:
            assert int(str(json.loads(r.stdout)['status']),0)==0,'unauthorized setup transaction succeeded'
        after=request(base+'/api/status')[1]['defaults']
        assert after['budget']==seed['budget'],after
        return {'unauthorized_change_rejected':True}
    case('initializer_only_configuration',unauthorized)
    def no_plain_commit():
        r=cast('send','--no-proxy','--json','--rpc-url',dep['endpoint'],'--private-key',
            '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80',
            '--gas-limit','1000000',dep['contract'],'commit_result(string)','{}',check=False)
        if r.returncode==0:assert int(str(json.loads(r.stdout)['status']),0)==0,'plain commit unexpectedly accepted'
        return {'plain_eth_commit_not_exported':True}
    case('plain_commit_route_unavailable',no_plain_commit)
    if args.faults:
        status=request(base+'/api/status')[1]
        assert status.get('test_controls'),'--faults requires test-controls build, never production'
        def set_fault(node,mode):
            r=cast('call','--no-proxy','--rpc-url',f'ws://127.0.0.1:{10087+1000*node}/ws',
                   dep['contract'],'set_fault(string)(bool)',mode)
            assert 'true' in r.stdout or int(r.stdout.strip(),16)==1,r.stdout
        def reset():
            for n in range(4):set_fault(n,'')
        def fault_case(modes,succeeds):
            reset()
            try:
                for node,mode in modes.items():set_fault(node,mode)
                rpc('anvil_mine',['0x3'])
                to=int(rpc('eth_blockNumber',[]),16)-2
                fq=dict(q);fq['to_block']=to
                return query(fq,seed['expected_total'],seed['expected_count'],succeeds)
            finally:reset()
        case('one_wrong_total_three_correct',lambda:fault_case({3:'wrong_total'},True))
        case('one_stale_snapshot_three_current',lambda:fault_case({3:'stale_snapshot'},True))
        case('one_unavailable_three_available',lambda:fault_case({3:'unavailable'},True))
        case('two_unavailable_no_quorum',lambda:fault_case({2:'unavailable',3:'unavailable'},False))
        case('two_wrong_totals_no_matching_quorum',lambda:fault_case({2:'wrong_total',3:'wrong_total'},False))
    output=ROOT/'output/results';output.mkdir(parents=True,exist_ok=True)
    report={'kind':'real-four-node-devnet-integration','test_controls':args.faults,'deployment':dep,
            'seed':seed,'tests':rows,'passed':sum(r['passed'] for r in rows),'total':len(rows)}
    name='integration-faults.json' if args.faults else 'integration.json'
    (output/name).write_text(json.dumps(report,indent=2)+'\n')
    print(f"{report['passed']}/{report['total']} passed; {output/name}")
    return 0 if all(r['passed'] for r in rows) else 1
if __name__=='__main__':
    try:sys.exit(main())
    except Exception as e:print('Integration setup failed:',e,file=sys.stderr);sys.exit(1)
